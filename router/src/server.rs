use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{Result, RouterError};
use crate::protocol::{
    decode_frame, decode_query_body, decode_register_body, encode_error_body, encode_frame,
    encode_query_exists_response, encode_query_get_response, encode_register_ack_body, Envelope,
    ErrorBody, FrameParts, PeerClass, QueryBody, RegisterAckBody, KIND_CALL, KIND_DISCONNECT,
    KIND_ERROR, KIND_HEARTBEAT, KIND_QUERY, KIND_REGISTER, KIND_RESPONSE, PROTOCOL_VERSION,
    ROUTER_PEER_ID,
};
use crate::registry::{PeerRegistry, PeerSender};

pub async fn serve(listener: TcpListener, registry: PeerRegistry) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let registry = registry.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_connection(stream, registry).await {
                eprintln!("multifrost-router connection error: {err}");
            }
        });
    }
}

async fn handle_connection(stream: TcpStream, registry: PeerRegistry) -> Result<()> {
    let websocket = accept_async(stream).await?;
    let (sink, mut source) = websocket.split();
    let sink = Arc::new(Mutex::new(sink));
    let mut registered_peer_id: Option<String> = None;

    let result: Result<()> = async {
        let first = match source.next().await {
            Some(Ok(message)) => message,
            Some(Err(err)) => return Err(err.into()),
            None => return Err(RouterError::ConnectionClosed),
        };

        let first_bytes = crate::protocol::ensure_binary_ws_message(first)?;
        let first_frame = decode_frame(&first_bytes)?;
        if first_frame.envelope.kind != KIND_REGISTER {
            send_error_and_close(
                sink.clone(),
                &first_frame.envelope.msg_id,
                "register.required",
                "first message must be register",
            )
            .await?;
            return Ok(());
        }

        let register = decode_register_body(&first_frame.body_bytes)?;
        validate_register_envelope(&first_frame.envelope, &register)?;

        match registry
            .insert_live_peer(
                register.peer_id.clone(),
                register.class.clone(),
                sink.clone(),
            )
            .await
        {
            Ok(()) => {
                registered_peer_id = Some(register.peer_id.clone());
                send_register_ack(sink.clone(), &first_frame.envelope.msg_id, &register).await?;
            }
            Err(RouterError::DuplicatePeerId(peer_id)) => {
                send_error_and_close(
                    sink.clone(),
                    &first_frame.envelope.msg_id,
                    "duplicate_peer_id",
                    &format!("peer_id {peer_id} is already live"),
                )
                .await?;
                return Ok(());
            }
            Err(err) => return Err(err),
        }

        let peer_id = register.peer_id;
        let peer_class = register.class;

        while let Some(message) = source.next().await {
            match message {
                Ok(Message::Binary(bytes)) => {
                    let raw = bytes.to_vec();
                    let FrameParts {
                        envelope,
                        body_bytes,
                    } = decode_frame(&raw)?;
                    if envelope.from != peer_id {
                        send_peer_error(
                            sink.clone(),
                            envelope.msg_id.as_str(),
                            "invalid_source",
                            "envelope.from does not match registered peer",
                        )
                        .await?;
                        continue;
                    }

                    match envelope.kind.as_str() {
                        KIND_QUERY => {
                            handle_query(
                                sink.clone(),
                                registry.clone(),
                                peer_id.as_str(),
                                envelope,
                                body_bytes,
                            )
                            .await?
                        }
                        KIND_CALL => {
                            handle_call(
                                sink.clone(),
                                registry.clone(),
                                peer_class.clone(),
                                envelope,
                                raw,
                            )
                            .await?
                        }
                        KIND_RESPONSE => {
                            handle_response(
                                sink.clone(),
                                registry.clone(),
                                peer_class.clone(),
                                envelope,
                                raw,
                            )
                            .await?
                        }
                        KIND_ERROR => {
                            handle_error(
                                sink.clone(),
                                registry.clone(),
                                peer_class.clone(),
                                envelope,
                                raw,
                            )
                            .await?
                        }
                        KIND_HEARTBEAT => {
                            handle_heartbeat(
                                sink.clone(),
                                registry.clone(),
                                peer_id.as_str(),
                                envelope,
                                raw,
                            )
                            .await?
                        }
                        KIND_DISCONNECT => {
                            handle_disconnect(
                                sink.clone(),
                                registry.clone(),
                                peer_id.as_str(),
                                envelope,
                                body_bytes,
                            )
                            .await?;
                            break;
                        }
                        other => {
                            send_peer_error(
                                sink.clone(),
                                envelope.msg_id.as_str(),
                                "unsupported_kind",
                                &format!("unsupported message kind: {other}"),
                            )
                            .await?;
                        }
                    }
                }
                Ok(Message::Close(_)) => break,
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
                Ok(_) => {
                    send_peer_error(
                        sink.clone(),
                        "unknown",
                        "non_binary_message",
                        "router only accepts binary websocket messages",
                    )
                    .await?;
                    break;
                }
                Err(err) => return Err(err.into()),
            }
        }

        Ok(())
    }
    .await;

    if let Some(peer_id) = registered_peer_id {
        registry.remove_by_peer_id(&peer_id).await;
    }

    result
}

fn validate_register_envelope(
    envelope: &Envelope,
    register: &crate::protocol::RegisterBody,
) -> Result<()> {
    if envelope.to != ROUTER_PEER_ID {
        return Err(RouterError::MalformedRegisterBody(
            "register target must be router".into(),
        ));
    }
    if envelope.from != register.peer_id {
        return Err(RouterError::MalformedRegisterBody(
            "register body peer_id must match envelope.from".into(),
        ));
    }
    Ok(())
}

async fn handle_query(
    sink: PeerSender,
    registry: PeerRegistry,
    current_peer_id: &str,
    envelope: Envelope,
    body_bytes: Vec<u8>,
) -> Result<()> {
    let body = decode_query_body(&body_bytes)?;
    let response = match body {
        QueryBody::PeerExists { peer_id } => {
            let snapshot = registry.lookup_by_peer_id(&peer_id).await;
            let exists = snapshot.is_some();
            let class = snapshot.as_ref().map(|value| value.class.clone());
            encode_query_exists_response(&peer_id, exists, class, exists)?
        }
        QueryBody::PeerGet { peer_id } => {
            let snapshot = registry.lookup_by_peer_id(&peer_id).await;
            let exists = snapshot.is_some();
            let class = snapshot.as_ref().map(|value| value.class.clone());
            encode_query_get_response(
                &peer_id,
                exists,
                class,
                snapshot
                    .as_ref()
                    .map(|value| value.connected)
                    .unwrap_or(false),
            )?
        }
    };

    let response_envelope = Envelope {
        v: envelope.v,
        kind: KIND_RESPONSE.to_string(),
        msg_id: envelope.msg_id,
        from: ROUTER_PEER_ID.to_string(),
        to: current_peer_id.to_string(),
        ts: envelope.ts,
    };
    send_frame(sink, &response_envelope, response).await?;
    Ok(())
}

async fn handle_call(
    sink: PeerSender,
    registry: PeerRegistry,
    current_class: PeerClass,
    envelope: Envelope,
    raw: Vec<u8>,
) -> Result<()> {
    if !matches!(current_class, PeerClass::Caller) {
        send_peer_error(
            sink,
            envelope.msg_id.as_str(),
            "invalid_source_class",
            "call traffic must originate from a caller peer",
        )
        .await?;
        return Ok(());
    }

    route_or_error(registry, envelope, raw, "call", sink).await
}

async fn handle_response(
    sink: PeerSender,
    registry: PeerRegistry,
    current_class: PeerClass,
    envelope: Envelope,
    raw: Vec<u8>,
) -> Result<()> {
    if !matches!(current_class, PeerClass::Service) {
        send_peer_error(
            sink,
            envelope.msg_id.as_str(),
            "invalid_source_class",
            "response traffic must originate from a service peer",
        )
        .await?;
        return Ok(());
    }

    route_to_target(registry, envelope, raw, sink, "response").await
}

async fn handle_error(
    sink: PeerSender,
    registry: PeerRegistry,
    _current_class: PeerClass,
    envelope: Envelope,
    raw: Vec<u8>,
) -> Result<()> {
    route_to_target(registry, envelope, raw, sink, "error").await
}

async fn handle_heartbeat(
    sink: PeerSender,
    registry: PeerRegistry,
    current_peer_id: &str,
    envelope: Envelope,
    raw: Vec<u8>,
) -> Result<()> {
    registry.mark_heartbeat(current_peer_id).await;
    if registry.snapshot_peer_exists(&envelope.to).await {
        route_to_target(registry, envelope, raw, sink, "heartbeat").await
    } else {
        let echoed_envelope = build_envelope(
            KIND_HEARTBEAT,
            envelope.msg_id,
            ROUTER_PEER_ID.to_string(),
            current_peer_id.to_string(),
            envelope.ts,
        );
        send_frame(
            sink,
            &echoed_envelope,
            encode_register_ack_body(&build_ack_body())?,
        )
        .await
    }
}

async fn handle_disconnect(
    sink: PeerSender,
    registry: PeerRegistry,
    current_peer_id: &str,
    envelope: Envelope,
    _body_bytes: Vec<u8>,
) -> Result<()> {
    registry.remove_by_peer_id(current_peer_id).await;
    let response_envelope = build_envelope(
        KIND_RESPONSE,
        envelope.msg_id,
        ROUTER_PEER_ID.to_string(),
        current_peer_id.to_string(),
        envelope.ts,
    );
    send_frame(
        sink,
        &response_envelope,
        encode_register_ack_body(&build_ack_body())?,
    )
    .await
}

async fn route_or_error(
    registry: PeerRegistry,
    envelope: Envelope,
    raw: Vec<u8>,
    kind: &str,
    sink: PeerSender,
) -> Result<()> {
    match decide_route_target(kind, registry.snapshot_peer_class(&envelope.to).await) {
        RouteTargetDecision::Deliver => {
            route_to_target(registry, envelope, raw, sink, kind).await?;
        }
        RouteTargetDecision::Reject { code, message } => {
            send_peer_error(sink, envelope.msg_id.as_str(), code, &message).await?;
        }
    }
    Ok(())
}

async fn route_to_target(
    registry: PeerRegistry,
    envelope: Envelope,
    raw: Vec<u8>,
    sink: PeerSender,
    kind: &str,
) -> Result<()> {
    if !registry.snapshot_peer_exists(&envelope.to).await {
        send_peer_error(
            sink,
            envelope.msg_id.as_str(),
            "peer_not_found",
            &format!("{kind} target not found"),
        )
        .await?;
        return Ok(());
    }

    let Some(target_sink) = registry.sender_by_peer_id(&envelope.to).await else {
        send_peer_error(
            sink,
            envelope.msg_id.as_str(),
            "peer_not_found",
            &format!("{kind} target not found"),
        )
        .await?;
        return Ok(());
    };

    match send_raw(target_sink.clone(), raw).await {
        Ok(()) => Ok(()),
        Err(_) => {
            registry.remove_by_peer_id(&envelope.to).await;
            send_peer_error(
                sink,
                envelope.msg_id.as_str(),
                "peer_not_found",
                &format!("{kind} target disconnected"),
            )
            .await
        }
    }
}

async fn send_register_ack(
    sink: PeerSender,
    msg_id: &str,
    register: &crate::protocol::RegisterBody,
) -> Result<()> {
    let envelope = build_envelope(
        KIND_RESPONSE,
        msg_id.to_string(),
        ROUTER_PEER_ID.to_string(),
        register.peer_id.clone(),
        now_ts(),
    );
    send_frame(
        sink,
        &envelope,
        encode_register_ack_body(&build_ack_body())?,
    )
    .await
}

async fn send_error_and_close(
    sink: PeerSender,
    msg_id: &str,
    code: &str,
    message: &str,
) -> Result<()> {
    let envelope = build_envelope(
        KIND_ERROR,
        msg_id.to_string(),
        ROUTER_PEER_ID.to_string(),
        ROUTER_PEER_ID.to_string(),
        now_ts(),
    );
    send_frame(
        sink,
        &envelope,
        encode_error_body(&build_error_body(code, message))?,
    )
    .await
}

async fn send_peer_error(sink: PeerSender, msg_id: &str, code: &str, message: &str) -> Result<()> {
    let envelope = build_envelope(
        KIND_ERROR,
        msg_id.to_string(),
        ROUTER_PEER_ID.to_string(),
        ROUTER_PEER_ID.to_string(),
        now_ts(),
    );
    send_frame(
        sink,
        &envelope,
        encode_error_body(&build_error_body(code, message))?,
    )
    .await
}

fn build_envelope(kind: &str, msg_id: String, from: String, to: String, ts: f64) -> Envelope {
    Envelope {
        v: PROTOCOL_VERSION,
        kind: kind.to_string(),
        msg_id,
        from,
        to,
        ts,
    }
}

fn build_ack_body() -> RegisterAckBody {
    RegisterAckBody {
        accepted: true,
        reason: None,
    }
}

fn build_error_body(code: &str, message: &str) -> ErrorBody {
    ErrorBody {
        code: code.to_string(),
        message: message.to_string(),
        kind: "router".to_string(),
        stack: None,
        details: None,
    }
}

enum RouteTargetDecision {
    Deliver,
    Reject { code: &'static str, message: String },
}

fn decide_route_target(kind: &str, target_class: Option<PeerClass>) -> RouteTargetDecision {
    match target_class {
        None => RouteTargetDecision::Reject {
            code: "peer_not_found",
            message: format!("{kind} target not found"),
        },
        Some(PeerClass::Caller) if kind == KIND_CALL => RouteTargetDecision::Reject {
            code: "invalid_target_class",
            message: "invalid target class".to_string(),
        },
        Some(PeerClass::Service) if kind == KIND_RESPONSE => RouteTargetDecision::Reject {
            code: "invalid_target_class",
            message: "responses must target a caller peer".to_string(),
        },
        Some(_) => RouteTargetDecision::Deliver,
    }
}

async fn send_frame(sink: PeerSender, envelope: &Envelope, body_bytes: Vec<u8>) -> Result<()> {
    let frame = encode_frame(envelope, &body_bytes)?;
    send_raw(sink, frame).await
}

async fn send_raw(sink: PeerSender, raw: Vec<u8>) -> Result<()> {
    sink.lock()
        .await
        .send(Message::Binary(raw.into()))
        .await
        .map_err(RouterError::from)
}

fn now_ts() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_register_envelope_accepts_matching_router_registration() {
        let envelope = build_envelope(
            KIND_REGISTER,
            "msg-1".to_string(),
            "peer-a".to_string(),
            ROUTER_PEER_ID.to_string(),
            1.0,
        );
        let register = crate::protocol::RegisterBody {
            peer_id: "peer-a".to_string(),
            class: PeerClass::Service,
        };

        assert!(validate_register_envelope(&envelope, &register).is_ok());
    }

    #[test]
    fn validate_register_envelope_rejects_wrong_target() {
        let envelope = build_envelope(
            KIND_REGISTER,
            "msg-1".to_string(),
            "peer-a".to_string(),
            "not-router".to_string(),
            1.0,
        );
        let register = crate::protocol::RegisterBody {
            peer_id: "peer-a".to_string(),
            class: PeerClass::Service,
        };

        let err = validate_register_envelope(&envelope, &register).unwrap_err();
        assert!(matches!(
            err,
            crate::error::RouterError::MalformedRegisterBody(_)
        ));
    }

    #[test]
    fn validate_register_envelope_rejects_peer_id_mismatch() {
        let envelope = build_envelope(
            KIND_REGISTER,
            "msg-1".to_string(),
            "peer-a".to_string(),
            ROUTER_PEER_ID.to_string(),
            1.0,
        );
        let register = crate::protocol::RegisterBody {
            peer_id: "peer-b".to_string(),
            class: PeerClass::Service,
        };

        let err = validate_register_envelope(&envelope, &register).unwrap_err();
        assert!(matches!(
            err,
            crate::error::RouterError::MalformedRegisterBody(_)
        ));
    }

    #[test]
    fn decide_route_target_enforces_class_rules() {
        assert!(matches!(
            decide_route_target(KIND_CALL, Some(PeerClass::Service)),
            RouteTargetDecision::Deliver
        ));
        assert!(matches!(
            decide_route_target(KIND_CALL, Some(PeerClass::Caller)),
            RouteTargetDecision::Reject {
                code: "invalid_target_class",
                ..
            }
        ));
        assert!(matches!(
            decide_route_target(KIND_RESPONSE, Some(PeerClass::Caller)),
            RouteTargetDecision::Deliver
        ));
        assert!(matches!(
            decide_route_target(KIND_RESPONSE, Some(PeerClass::Service)),
            RouteTargetDecision::Reject {
                code: "invalid_target_class",
                ..
            }
        ));
        assert!(matches!(
            decide_route_target(KIND_ERROR, None),
            RouteTargetDecision::Reject {
                code: "peer_not_found",
                ..
            }
        ));
    }

    #[test]
    fn build_envelope_sets_router_metadata() {
        let envelope = build_envelope(
            KIND_ERROR,
            "msg-9".to_string(),
            "from-peer".to_string(),
            "to-peer".to_string(),
            42.5,
        );

        assert_eq!(envelope.v, PROTOCOL_VERSION);
        assert_eq!(envelope.kind, KIND_ERROR);
        assert_eq!(envelope.msg_id, "msg-9");
        assert_eq!(envelope.from, "from-peer");
        assert_eq!(envelope.to, "to-peer");
        assert_eq!(envelope.ts, 42.5);
    }
}
