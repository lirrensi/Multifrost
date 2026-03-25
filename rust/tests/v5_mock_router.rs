use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use multifrost::{
    call, CallBody, ConnectOptions, Connection, Envelope, ErrorBody, FrameParts, MultifrostError,
    PeerClass, QueryBody, QueryExistsResponseBody, QueryGetResponseBody, RegisterAckBody,
    RegisterBody, ResponseBody,
};
use rmp_serde::{from_slice, to_vec_named};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

type ServerWs = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

const KIND_REGISTER: &str = "register";
const KIND_QUERY: &str = "query";
const KIND_CALL: &str = "call";
const KIND_RESPONSE: &str = "response";
const KIND_ERROR: &str = "error";
const KIND_DISCONNECT: &str = "disconnect";
const ROUTER_PEER_ID: &str = "router";
const PROTOCOL_VERSION: u32 = 5;

#[derive(Clone)]
struct CallerRouterConfig {
    register_accepted: bool,
    call_mode: CallMode,
}

#[derive(Clone, Copy)]
enum CallMode {
    Success,
    RemoteError,
    CloseOnCall,
}

async fn start_caller_router(
    config: CallerRouterConfig,
) -> (u16, tokio::task::JoinHandle<()>, oneshot::Receiver<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (disconnect_tx, disconnect_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let connection_index = Arc::new(AtomicUsize::new(0));
        let mut disconnect_tx = Some(disconnect_tx);

        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let idx = connection_index.fetch_add(1, Ordering::SeqCst);

            if idx == 0 {
                tokio::spawn(async move {
                    let _ = accept_async(stream).await;
                    tokio::time::sleep(Duration::from_millis(200)).await;
                });
                continue;
            }

            let config = config.clone();
            let disconnect_tx = disconnect_tx.take();
            tokio::spawn(async move {
                let ws = accept_async(stream).await.unwrap();
                handle_caller_connection(ws, config, disconnect_tx).await;
            });
        }
    });

    (port, handle, disconnect_rx)
}

async fn handle_caller_connection(
    mut ws: ServerWs,
    config: CallerRouterConfig,
    disconnect_tx: Option<oneshot::Sender<()>>,
) {
    let register_frame = read_binary_frame(&mut ws).await;
    assert_eq!(register_frame.envelope.kind, KIND_REGISTER);
    let register_body: RegisterBody = decode_body(&register_frame.body_bytes);

    let ack = RegisterAckBody {
        accepted: config.register_accepted,
        reason: if config.register_accepted {
            None
        } else {
            Some("duplicate peer_id".into())
        },
    };
    let ack_envelope = build_envelope(
        KIND_RESPONSE,
        ROUTER_PEER_ID,
        &register_body.peer_id,
        &register_frame.envelope.msg_id,
    );
    let ack_frame = encode_frame(&ack_envelope, &encode_body(&ack));
    ws.send(Message::Binary(Bytes::from(ack_frame)))
        .await
        .unwrap();

    if !config.register_accepted {
        return;
    }

    while let Some(message) = ws.next().await {
        let Ok(message) = message else {
            break;
        };

        match message {
            Message::Binary(bytes) => {
                let frame = decode_frame(bytes.as_ref());
                match frame.envelope.kind.as_str() {
                    KIND_QUERY => {
                        let query: QueryBody = decode_body(&frame.body_bytes);
                        let body = match query {
                            QueryBody::PeerExists { peer_id } => {
                                let response = QueryExistsResponseBody {
                                    peer_id,
                                    exists: true,
                                    class: Some(PeerClass::Service),
                                    connected: true,
                                };
                                let envelope = build_envelope(
                                    KIND_RESPONSE,
                                    ROUTER_PEER_ID,
                                    frame.envelope.from.as_str(),
                                    frame.envelope.msg_id.as_str(),
                                );
                                encode_frame(&envelope, &encode_body(&response))
                            }
                            QueryBody::PeerGet { peer_id } => {
                                let response = QueryGetResponseBody {
                                    peer_id,
                                    exists: true,
                                    class: Some(PeerClass::Service),
                                    connected: true,
                                };
                                let envelope = build_envelope(
                                    KIND_RESPONSE,
                                    ROUTER_PEER_ID,
                                    frame.envelope.from.as_str(),
                                    frame.envelope.msg_id.as_str(),
                                );
                                encode_frame(&envelope, &encode_body(&response))
                            }
                        };
                        ws.send(Message::Binary(Bytes::from(body))).await.unwrap();
                    }
                    KIND_CALL => {
                        let call: CallBody = decode_body(&frame.body_bytes);
                        let response = match config.call_mode {
                            CallMode::Success => {
                                assert_eq!(call.function, "add");
                                assert_eq!(call.args, vec![json!(10), json!(20)]);
                                let body = ResponseBody { result: json!(30) };
                                let envelope = build_envelope(
                                    KIND_RESPONSE,
                                    frame.envelope.to.as_str(),
                                    frame.envelope.from.as_str(),
                                    frame.envelope.msg_id.as_str(),
                                );
                                Some(encode_frame(&envelope, &encode_body(&body)))
                            }
                            CallMode::RemoteError => {
                                let body = ErrorBody {
                                    code: "boom".into(),
                                    message: format!("remote failure: {}", call.function),
                                    kind: "application".into(),
                                    stack: None,
                                    details: None,
                                };
                                let envelope = build_envelope(
                                    KIND_ERROR,
                                    frame.envelope.to.as_str(),
                                    frame.envelope.from.as_str(),
                                    frame.envelope.msg_id.as_str(),
                                );
                                Some(encode_frame(&envelope, &encode_body(&body)))
                            }
                            CallMode::CloseOnCall => {
                                let _ = ws.close(None).await;
                                None
                            }
                        };
                        if let Some(response) = response {
                            ws.send(Message::Binary(Bytes::from(response)))
                                .await
                                .unwrap();
                        } else {
                            break;
                        }
                    }
                    KIND_DISCONNECT => {
                        if let Some(tx) = disconnect_tx {
                            let _ = tx.send(());
                        }
                        break;
                    }
                    _ => {}
                }
            }
            Message::Close(_) => break,
            Message::Ping(payload) => {
                let _ = ws.send(Message::Pong(payload)).await;
            }
            Message::Pong(_) => {}
            _ => {}
        }
    }
}

async fn start_caller_success_router() -> (u16, tokio::task::JoinHandle<()>, oneshot::Receiver<()>)
{
    start_caller_router(CallerRouterConfig {
        register_accepted: true,
        call_mode: CallMode::Success,
    })
    .await
}

async fn start_caller_error_router() -> (u16, tokio::task::JoinHandle<()>, oneshot::Receiver<()>) {
    start_caller_router(CallerRouterConfig {
        register_accepted: true,
        call_mode: CallMode::RemoteError,
    })
    .await
}

async fn start_caller_close_router() -> (u16, tokio::task::JoinHandle<()>, oneshot::Receiver<()>) {
    start_caller_router(CallerRouterConfig {
        register_accepted: true,
        call_mode: CallMode::CloseOnCall,
    })
    .await
}

async fn start_caller_reject_router() -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        let mut connection_index = 0usize;
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let idx = connection_index;
            connection_index += 1;
            tokio::spawn(async move {
                let mut ws = accept_async(stream).await.unwrap();
                if idx == 0 {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    return;
                }

                let register_frame = read_binary_frame(&mut ws).await;
                let register_body: RegisterBody = decode_body(&register_frame.body_bytes);
                let ack = RegisterAckBody {
                    accepted: false,
                    reason: Some("duplicate peer_id".into()),
                };
                let ack_envelope = build_envelope(
                    KIND_RESPONSE,
                    ROUTER_PEER_ID,
                    &register_body.peer_id,
                    &register_frame.envelope.msg_id,
                );
                let ack_frame = encode_frame(&ack_envelope, &encode_body(&ack));
                ws.send(Message::Binary(Bytes::from(ack_frame)))
                    .await
                    .unwrap();
            });
        }
    });

    (port, handle)
}

async fn read_binary_frame(ws: &mut ServerWs) -> FrameParts {
    let message = timeout(Duration::from_secs(2), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    match message {
        Message::Binary(bytes) => decode_frame(bytes.as_ref()),
        other => panic!("expected binary message, got {other:?}"),
    }
}

fn build_envelope(kind: &str, from: &str, to: &str, msg_id: &str) -> Envelope {
    Envelope {
        v: PROTOCOL_VERSION,
        kind: kind.to_string(),
        msg_id: msg_id.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        ts: 0.0,
    }
}

fn encode_frame(envelope: &Envelope, body_bytes: &[u8]) -> Vec<u8> {
    let envelope_bytes = to_vec_named(envelope).unwrap();
    let mut out = Vec::with_capacity(4 + envelope_bytes.len() + body_bytes.len());
    out.extend_from_slice(&(envelope_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(&envelope_bytes);
    out.extend_from_slice(body_bytes);
    out
}

fn decode_frame(bytes: &[u8]) -> FrameParts {
    let envelope_len = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let envelope: Envelope = from_slice(&bytes[4..4 + envelope_len]).unwrap();
    let body_bytes = bytes[4 + envelope_len..].to_vec();
    FrameParts {
        envelope,
        body_bytes,
    }
}

fn encode_body<T: serde::Serialize>(value: &T) -> Vec<u8> {
    to_vec_named(value).unwrap()
}

fn decode_body<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> T {
    from_slice(bytes).unwrap()
}

#[tokio::test]
async fn connection_connect_call_query_and_disconnect_against_mock_router() {
    let (port, handle, disconnect_rx) = start_caller_success_router().await;
    let connection = Connection::connect_with_options(
        "math-service",
        ConnectOptions {
            caller_peer_id: Some("caller-1".into()),
            request_timeout: Some(Duration::from_secs(2)),
            router_port: Some(port),
        },
    )
    .await
    .unwrap();
    let handle_ref = connection.handle();

    let sum: i64 = call!(handle_ref, add(10, 20)).await.unwrap();
    assert_eq!(sum, 30);

    let exists = handle_ref.query_peer_exists("math-service").await.unwrap();
    assert!(exists);

    let peer = handle_ref.query_peer_get("math-service").await.unwrap();
    assert!(peer.exists);
    assert_eq!(peer.class, Some(PeerClass::Service));

    handle_ref.stop().await;
    timeout(Duration::from_secs(2), disconnect_rx)
        .await
        .unwrap()
        .unwrap();
    handle.abort();
}

#[tokio::test]
async fn connection_connect_surfaces_register_rejection() {
    let (port, handle) = start_caller_reject_router().await;
    let result = Connection::connect_with_options(
        "math-service",
        ConnectOptions {
            caller_peer_id: Some("caller-reject".into()),
            request_timeout: Some(Duration::from_secs(2)),
            router_port: Some(port),
        },
    )
    .await;
    let err = match result {
        Ok(_) => panic!("expected register rejection"),
        Err(err) => err,
    };
    assert!(matches!(err, MultifrostError::RegisterRejected(_)));
    handle.abort();
}

#[tokio::test]
async fn connection_connect_surfaces_remote_call_error() {
    let (port, handle, disconnect_rx) = start_caller_error_router().await;
    let connection = Connection::connect_with_options(
        "math-service",
        ConnectOptions {
            caller_peer_id: Some("caller-error".into()),
            request_timeout: Some(Duration::from_secs(2)),
            router_port: Some(port),
        },
    )
    .await
    .unwrap();
    let handle_ref = connection.handle();

    let err = handle_ref
        .call_to::<i64>("math-service", "explode", vec![])
        .await
        .unwrap_err();
    assert!(matches!(err, MultifrostError::RemoteCallError(_)));

    handle_ref.stop().await;
    timeout(Duration::from_secs(2), disconnect_rx)
        .await
        .unwrap()
        .unwrap();
    handle.abort();
}

#[tokio::test]
async fn connection_connect_fails_pending_call_when_router_dies_mid_call() {
    let (port, handle, disconnect_rx) = start_caller_close_router().await;
    let connection = Connection::connect_with_options(
        "math-service",
        ConnectOptions {
            caller_peer_id: Some("caller-close".into()),
            request_timeout: None,
            router_port: Some(port),
        },
    )
    .await
    .unwrap();
    let handle_ref = connection.handle();

    let err = handle_ref
        .call_to::<i64>("math-service", "add", vec![json!(10), json!(20)])
        .await
        .unwrap_err();
    assert!(matches!(err, MultifrostError::RouterUnavailable(_)));

    handle_ref.stop().await;
    let _ = timeout(Duration::from_secs(2), disconnect_rx).await;
    handle.abort();
}
