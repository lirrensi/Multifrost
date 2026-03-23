use crate::error::{MultifrostError, Result};
use crate::message::{
    binary_message, build_call_envelope, build_disconnect_envelope, build_query_envelope,
    build_register_envelope, decode_error_body, decode_frame, decode_query_exists_response,
    decode_query_get_response, decode_register_ack_body, encode_call_body, encode_frame,
    encode_query_body, encode_register_body, FrameParts, PeerClass, QueryBody,
    QueryExistsResponseBody, QueryGetResponseBody, RegisterBody, KIND_ERROR, KIND_RESPONSE,
};
use crate::router_bootstrap::{connect_or_bootstrap, RouterBootstrapConfig};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;
type WsSink = futures_util::stream::SplitSink<WsStream, Message>;
type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<Result<FrameParts>>>>>;

#[derive(Debug, Clone)]
pub struct CallerTransportConfig {
    pub peer_id: String,
    pub default_target_peer_id: Option<String>,
    pub request_timeout: Option<Duration>,
    pub bootstrap: RouterBootstrapConfig,
}

#[derive(Clone)]
pub struct CallerTransport {
    inner: Arc<CallerTransportInner>,
}

struct CallerTransportInner {
    peer_id: String,
    default_target_peer_id: Option<String>,
    router_endpoint: String,
    sink: Arc<Mutex<WsSink>>,
    pending: PendingMap,
    request_timeout: Option<Duration>,
    inbound_task: Mutex<Option<JoinHandle<()>>>,
}

impl CallerTransport {
    pub async fn connect(config: CallerTransportConfig) -> Result<Self> {
        connect_or_bootstrap(&config.bootstrap).await?;
        let router_endpoint = config.bootstrap.endpoint.ws_url();
        let (websocket, _) = connect_async(&router_endpoint)
            .await
            .map_err(|err| MultifrostError::RouterUnavailable(err.to_string()))?;
        let (sink, mut source) = websocket.split();
        let sink = Arc::new(Mutex::new(sink));

        let register = RegisterBody {
            peer_id: config.peer_id.clone(),
            class: PeerClass::Caller,
        };
        let register_envelope = build_register_envelope(&config.peer_id, PeerClass::Caller);
        let register_frame = encode_frame(&register_envelope, &encode_register_body(&register)?)?;
        send_raw_frame(&sink, register_frame).await?;

        let ack = match source.next().await {
            Some(Ok(Message::Binary(bytes))) => {
                let frame = decode_frame(&bytes)?;
                if frame.envelope.kind != KIND_RESPONSE {
                    return Err(MultifrostError::RegisterRejected(format!(
                        "unexpected register reply kind: {}",
                        frame.envelope.kind
                    )));
                }
                decode_register_ack_body(&frame.body_bytes)?
            }
            Some(Ok(other)) => {
                return Err(MultifrostError::RegisterRejected(format!(
                    "unexpected register reply message: {other:?}"
                )))
            }
            Some(Err(err)) => return Err(MultifrostError::RouterUnavailable(err.to_string())),
            None => return Err(MultifrostError::RegisterRejected("router closed".into())),
        };

        if !ack.accepted {
            return Err(MultifrostError::RegisterRejected(
                ack.reason
                    .unwrap_or_else(|| "router rejected caller registration".into()),
            ));
        }

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let pending_loop = Arc::clone(&pending);
        let inbound_task = tokio::spawn(async move {
            inbound_loop(source, pending_loop).await;
        });

        Ok(Self {
            inner: Arc::new(CallerTransportInner {
                peer_id: config.peer_id,
                default_target_peer_id: config.default_target_peer_id,
                router_endpoint,
                sink,
                pending,
                request_timeout: config.request_timeout,
                inbound_task: Mutex::new(Some(inbound_task)),
            }),
        })
    }

    pub fn peer_id(&self) -> &str {
        &self.inner.peer_id
    }

    pub fn default_target_peer_id(&self) -> Option<&str> {
        self.inner.default_target_peer_id.as_deref()
    }

    pub async fn call<R>(&self, function: &str, args: Vec<serde_json::Value>) -> Result<R>
    where
        R: DeserializeOwned,
    {
        let target = self.default_target_peer_id().ok_or_else(|| {
            MultifrostError::ConfigError("no default target peer_id configured".into())
        })?;
        self.call_to(target, function, args).await
    }

    pub async fn call_to<R>(
        &self,
        target_peer_id: &str,
        function: &str,
        args: Vec<serde_json::Value>,
    ) -> Result<R>
    where
        R: DeserializeOwned,
    {
        let body = crate::message::CallBody {
            function: function.to_string(),
            args,
            namespace: None,
        };
        let body_bytes = encode_call_body(&body)?;
        let envelope = build_call_envelope(&self.inner.peer_id, target_peer_id);
        let frame = encode_frame(&envelope, &body_bytes)?;
        let frame = self.send_and_wait(frame, &envelope.msg_id).await?;

        if frame.envelope.kind == KIND_ERROR {
            return Err(decode_router_or_peer_error(&frame.body_bytes)?);
        }

        let response = crate::message::decode_response_body(&frame.body_bytes)?;
        serde_json::from_value(response.result).map_err(MultifrostError::JsonError)
    }

    pub async fn query_peer_exists(&self, peer_id: &str) -> Result<QueryExistsResponseBody> {
        let body = QueryBody::PeerExists {
            peer_id: peer_id.to_string(),
        };
        let body_bytes = encode_query_body(&body)?;
        let envelope = build_query_envelope(&self.inner.peer_id, crate::message::ROUTER_PEER_ID);
        let frame = encode_frame(&envelope, &body_bytes)?;
        let frame = self.send_and_wait(frame, &envelope.msg_id).await?;
        if frame.envelope.kind == KIND_ERROR {
            return Err(decode_router_or_peer_error(&frame.body_bytes)?);
        }
        decode_query_exists_response(&frame.body_bytes)
    }

    pub async fn query_peer_get(&self, peer_id: &str) -> Result<QueryGetResponseBody> {
        let body = QueryBody::PeerGet {
            peer_id: peer_id.to_string(),
        };
        let body_bytes = encode_query_body(&body)?;
        let envelope = build_query_envelope(&self.inner.peer_id, crate::message::ROUTER_PEER_ID);
        let frame = encode_frame(&envelope, &body_bytes)?;
        let frame = self.send_and_wait(frame, &envelope.msg_id).await?;
        if frame.envelope.kind == KIND_ERROR {
            return Err(decode_router_or_peer_error(&frame.body_bytes)?);
        }
        decode_query_get_response(&frame.body_bytes)
    }

    pub async fn disconnect(&self) -> Result<()> {
        let envelope = build_disconnect_envelope(
            &self.inner.peer_id,
            crate::message::ROUTER_PEER_ID,
            &crate::message::new_msg_id(),
        );
        let frame = encode_frame(&envelope, &[])?;
        let _ = send_raw_frame(&self.inner.sink, frame).await;
        if let Some(task) = self.inner.inbound_task.lock().await.take() {
            task.abort();
        }
        Ok(())
    }

    async fn send_and_wait(&self, frame: Vec<u8>, msg_id: &str) -> Result<FrameParts> {
        let (tx, rx) = oneshot::channel();
        self.inner
            .pending
            .lock()
            .await
            .insert(msg_id.to_string(), tx);

        if let Err(err) = send_raw_frame(&self.inner.sink, frame).await {
            self.inner.pending.lock().await.remove(msg_id);
            return Err(err);
        }

        let received = match self.inner.request_timeout {
            Some(duration) => timeout(duration, rx)
                .await
                .map_err(|_| MultifrostError::TimeoutError)?,
            None => rx.await,
        };

        match received {
            Ok(result) => result,
            Err(_closed) => Err(MultifrostError::RouterUnavailable(
                self.inner.router_endpoint.clone(),
            )),
        }
    }
}

async fn inbound_loop(
    mut source: futures_util::stream::SplitStream<WsStream>,
    pending: PendingMap,
) {
    while let Some(message) = source.next().await {
        let Ok(Message::Binary(bytes)) = message else {
            continue;
        };
        let Ok(frame) = decode_frame(&bytes) else {
            continue;
        };
        if let Some(sender) = pending.lock().await.remove(&frame.envelope.msg_id) {
            let _ = sender.send(Ok(frame));
        }
    }
}

async fn send_raw_frame(sink: &Arc<Mutex<WsSink>>, frame: Vec<u8>) -> Result<()> {
    let mut sink = sink.lock().await;
    sink.send(binary_message(frame))
        .await
        .map_err(|err| MultifrostError::WebSocketError(err.to_string()))
}

fn decode_router_or_peer_error(bytes: &[u8]) -> Result<MultifrostError> {
    let body = decode_error_body(bytes)?;
    match body.kind.as_str() {
        "router" => Ok(MultifrostError::RouterUnavailable(body.message)),
        "protocol" => Ok(MultifrostError::MalformedFrame(body.message)),
        "application" => Ok(MultifrostError::RemoteCallError(body.message)),
        "library" => Ok(MultifrostError::InvalidMessage(body.message)),
        _ => Ok(MultifrostError::RemoteCallError(body.message)),
    }
}

pub(crate) async fn send_disconnect_frame(sink: &Arc<Mutex<WsSink>>, peer_id: &str) -> Result<()> {
    let envelope = build_disconnect_envelope(
        peer_id,
        crate::message::ROUTER_PEER_ID,
        &crate::message::new_msg_id(),
    );
    let frame = encode_frame(&envelope, &[])?;
    send_raw_frame(sink, frame).await
}

pub(crate) async fn connect_ws(endpoint: &str) -> Result<WsStream> {
    let (ws, _) = connect_async(endpoint)
        .await
        .map_err(|err| MultifrostError::RouterUnavailable(err.to_string()))?;
    Ok(ws)
}
