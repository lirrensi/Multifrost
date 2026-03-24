use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use multifrost_router::protocol::{
    decode_error_body, decode_frame, encode_frame, encode_query_body, encode_register_body,
    Envelope, FrameParts, PeerClass, QueryBody, QueryGetResponseBody, RegisterBody, KIND_CALL,
    KIND_DISCONNECT, KIND_ERROR, KIND_QUERY, KIND_REGISTER, KIND_RESPONSE, PROTOCOL_VERSION,
    ROUTER_PEER_ID,
};
use multifrost_router::registry::PeerRegistry;
use multifrost_router::server;

async fn start_test_router() -> (String, JoinHandle<()>, PeerRegistry) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let registry = PeerRegistry::new();
    let registry_for_task = registry.clone();
    let handle = tokio::spawn(async move {
        let _ = server::serve(listener, registry_for_task).await;
    });
    (format!("ws://{}", addr), handle, registry)
}

async fn connect_peer(
    base_url: &str,
    peer_id: &str,
    class: PeerClass,
) -> (
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Envelope,
) {
    let (mut ws, _) = connect_async(base_url).await.unwrap();
    let register = RegisterBody {
        peer_id: peer_id.to_string(),
        class,
    };
    let envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_REGISTER.to_string(),
        msg_id: format!("{peer_id}-register"),
        from: peer_id.to_string(),
        to: ROUTER_PEER_ID.to_string(),
        ts: 1_700_000_000.0,
    };
    let body = encode_register_body(&register).unwrap();
    let frame = encode_frame(&envelope, &body).unwrap();
    ws.send(Message::Binary(Bytes::from(frame))).await.unwrap();
    (ws, envelope)
}

async fn read_binary_frame(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> FrameParts {
    let message = timeout(Duration::from_secs(2), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let bytes = match message {
        Message::Binary(bytes) => bytes,
        other => panic!("expected binary message, got {other:?}"),
    };
    decode_frame(bytes.as_ref()).unwrap()
}

async fn expect_connection_closed(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    match timeout(Duration::from_secs(2), ws.next()).await {
        Ok(Some(Ok(Message::Close(_)))) | Ok(None) => {}
        Ok(Some(Ok(Message::Binary(bytes)))) => {
            panic!(
                "expected websocket close, got binary frame: {:?}",
                decode_frame(bytes.as_ref()).ok()
            )
        }
        Ok(Some(Ok(other))) => panic!("expected websocket close, got {other:?}"),
        Ok(Some(Err(_))) => {}
        Err(_) => panic!("timed out waiting for websocket close"),
    }
}

async fn wait_for_peer_absent(registry: &PeerRegistry, peer_id: &str) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if !registry.snapshot_peer_exists(peer_id).await {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("peer {peer_id} was still registered after waiting");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn successful_service_register_on_default_port_override_test_server() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut ws, _) = connect_peer(&base_url, "service-a", PeerClass::Service).await;
    let ack = read_binary_frame(&mut ws).await;
    assert_eq!(ack.envelope.kind, KIND_RESPONSE);
    assert_eq!(ack.envelope.to, "service-a");
    handle.abort();
}

#[tokio::test]
async fn successful_caller_register() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut ws, _) = connect_peer(&base_url, "caller-a", PeerClass::Caller).await;
    let ack = read_binary_frame(&mut ws).await;
    assert_eq!(ack.envelope.kind, KIND_RESPONSE);
    assert_eq!(ack.envelope.to, "caller-a");
    handle.abort();
}

#[tokio::test]
async fn duplicate_live_peer_id_rejection() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut ws1, _) = connect_peer(&base_url, "dupe", PeerClass::Service).await;
    let _ = read_binary_frame(&mut ws1).await;
    let (mut ws2, _) = connect_peer(&base_url, "dupe", PeerClass::Service).await;
    let rejected = read_binary_frame(&mut ws2).await;
    assert_eq!(rejected.envelope.kind, KIND_ERROR);
    let error_body = decode_error_body(&rejected.body_bytes).unwrap();
    assert_eq!(error_body.code, "duplicate_peer_id");
    handle.abort();
}

#[tokio::test]
async fn query_peer_exists_for_present_service() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut service_ws, _) = connect_peer(&base_url, "service-q", PeerClass::Service).await;
    let _ = read_binary_frame(&mut service_ws).await;
    let (mut caller_ws, _) = connect_peer(&base_url, "caller-q", PeerClass::Caller).await;
    let _ = read_binary_frame(&mut caller_ws).await;

    let query = QueryBody::PeerExists {
        peer_id: "service-q".into(),
    };
    let envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_QUERY.to_string(),
        msg_id: "query-1".into(),
        from: "caller-q".into(),
        to: ROUTER_PEER_ID.to_string(),
        ts: 1_700_000_001.0,
    };
    let body = encode_query_body(&query).unwrap();
    let frame = encode_frame(&envelope, &body).unwrap();
    caller_ws
        .send(Message::Binary(Bytes::from(frame)))
        .await
        .unwrap();

    let reply = read_binary_frame(&mut caller_ws).await;
    assert_eq!(reply.envelope.kind, KIND_RESPONSE);
    assert_eq!(reply.envelope.to, "caller-q");
    handle.abort();
}

#[tokio::test]
async fn query_peer_exists_for_missing_peer() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut caller_ws, _) = connect_peer(&base_url, "caller-q2", PeerClass::Caller).await;
    let _ = read_binary_frame(&mut caller_ws).await;

    let query = QueryBody::PeerExists {
        peer_id: "missing".into(),
    };
    let envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_QUERY.to_string(),
        msg_id: "query-2".into(),
        from: "caller-q2".into(),
        to: ROUTER_PEER_ID.to_string(),
        ts: 1_700_000_001.0,
    };
    let body = encode_query_body(&query).unwrap();
    let frame = encode_frame(&envelope, &body).unwrap();
    caller_ws
        .send(Message::Binary(Bytes::from(frame)))
        .await
        .unwrap();

    let reply = read_binary_frame(&mut caller_ws).await;
    assert_eq!(reply.envelope.kind, KIND_RESPONSE);
    handle.abort();
}

#[tokio::test]
async fn query_peer_get_returns_peer_class_and_connected_state() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut service_ws, _) = connect_peer(&base_url, "service-get", PeerClass::Service).await;
    let _ = read_binary_frame(&mut service_ws).await;
    let (mut caller_ws, _) = connect_peer(&base_url, "caller-get", PeerClass::Caller).await;
    let _ = read_binary_frame(&mut caller_ws).await;

    let query = QueryBody::PeerGet {
        peer_id: "service-get".into(),
    };
    let envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_QUERY.to_string(),
        msg_id: "query-get-1".into(),
        from: "caller-get".into(),
        to: ROUTER_PEER_ID.to_string(),
        ts: 1_700_000_001.5,
    };
    let body = encode_query_body(&query).unwrap();
    let frame = encode_frame(&envelope, &body).unwrap();
    caller_ws
        .send(Message::Binary(Bytes::from(frame)))
        .await
        .unwrap();

    let reply = read_binary_frame(&mut caller_ws).await;
    assert_eq!(reply.envelope.kind, KIND_RESPONSE);
    let response: QueryGetResponseBody = rmp_serde::from_slice(&reply.body_bytes).unwrap();
    assert_eq!(response.peer_id, "service-get");
    assert!(response.exists);
    assert_eq!(response.class, Some(PeerClass::Service));
    assert!(response.connected);
    handle.abort();
}

#[tokio::test]
async fn call_routed_to_service_target_with_unchanged_body_bytes() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut service_ws, _) = connect_peer(&base_url, "service-call", PeerClass::Service).await;
    let _ = read_binary_frame(&mut service_ws).await;
    let (mut caller_ws, _) = connect_peer(&base_url, "caller-call", PeerClass::Caller).await;
    let _ = read_binary_frame(&mut caller_ws).await;

    let body_bytes = vec![0x82, 0xa3, b'f', b'o', b'o', 0x01];
    let envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_CALL.to_string(),
        msg_id: "call-1".into(),
        from: "caller-call".into(),
        to: "service-call".into(),
        ts: 1_700_000_002.0,
    };
    let frame = encode_frame(&envelope, &body_bytes).unwrap();
    caller_ws
        .send(Message::Binary(Bytes::from(frame)))
        .await
        .unwrap();

    let routed = read_binary_frame(&mut service_ws).await;
    assert_eq!(routed.envelope.kind, KIND_CALL);
    assert_eq!(routed.body_bytes, body_bytes);
    handle.abort();
}

#[tokio::test]
async fn call_to_missing_target_returns_router_error() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut caller_ws, _) = connect_peer(&base_url, "caller-missing", PeerClass::Caller).await;
    let _ = read_binary_frame(&mut caller_ws).await;

    let envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_CALL.to_string(),
        msg_id: "call-missing".into(),
        from: "caller-missing".into(),
        to: "does-not-exist".into(),
        ts: 1_700_000_003.0,
    };
    let frame = encode_frame(&envelope, &[0x80]).unwrap();
    caller_ws
        .send(Message::Binary(Bytes::from(frame)))
        .await
        .unwrap();

    let reply = read_binary_frame(&mut caller_ws).await;
    assert_eq!(reply.envelope.kind, KIND_ERROR);
    let error_body = decode_error_body(&reply.body_bytes).unwrap();
    assert_eq!(error_body.code, "peer_not_found");
    handle.abort();
}

#[tokio::test]
async fn response_routes_back_from_service_to_caller() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut service_ws, _) = connect_peer(&base_url, "service-response", PeerClass::Service).await;
    let _ = read_binary_frame(&mut service_ws).await;
    let (mut caller_ws, _) = connect_peer(&base_url, "caller-response", PeerClass::Caller).await;
    let _ = read_binary_frame(&mut caller_ws).await;

    let call_envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_CALL.to_string(),
        msg_id: "call-response-1".into(),
        from: "caller-response".into(),
        to: "service-response".into(),
        ts: 1_700_000_002.0,
    };
    let call_body = vec![0x80];
    let frame = encode_frame(&call_envelope, &call_body).unwrap();
    caller_ws
        .send(Message::Binary(Bytes::from(frame)))
        .await
        .unwrap();

    let routed_call = read_binary_frame(&mut service_ws).await;
    assert_eq!(routed_call.envelope.kind, KIND_CALL);
    assert_eq!(routed_call.body_bytes, call_body);

    let response_envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_RESPONSE.to_string(),
        msg_id: routed_call.envelope.msg_id.clone(),
        from: "service-response".into(),
        to: "caller-response".into(),
        ts: 1_700_000_002.5,
    };
    let response_body = vec![0x81, 0xa2, b'o', b'k', 0xc3];
    let response_frame = encode_frame(&response_envelope, &response_body).unwrap();
    service_ws
        .send(Message::Binary(Bytes::from(response_frame)))
        .await
        .unwrap();

    let routed_response = read_binary_frame(&mut caller_ws).await;
    assert_eq!(routed_response.envelope.kind, KIND_RESPONSE);
    assert_eq!(routed_response.body_bytes, response_body);
    handle.abort();
}

#[tokio::test]
async fn call_to_caller_target_returns_invalid_target_class_error() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut caller_target_ws, _) =
        connect_peer(&base_url, "caller-target", PeerClass::Caller).await;
    let _ = read_binary_frame(&mut caller_target_ws).await;
    let (mut caller_ws, _) = connect_peer(&base_url, "caller-origin", PeerClass::Caller).await;
    let _ = read_binary_frame(&mut caller_ws).await;

    let envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_CALL.to_string(),
        msg_id: "call-invalid-class".into(),
        from: "caller-origin".into(),
        to: "caller-target".into(),
        ts: 1_700_000_004.0,
    };
    let frame = encode_frame(&envelope, &[0x80]).unwrap();
    caller_ws
        .send(Message::Binary(Bytes::from(frame)))
        .await
        .unwrap();

    let reply = read_binary_frame(&mut caller_ws).await;
    assert_eq!(reply.envelope.kind, KIND_ERROR);
    let error_body = decode_error_body(&reply.body_bytes).unwrap();
    assert_eq!(error_body.code, "invalid_target_class");
    handle.abort();
}

#[tokio::test]
async fn malformed_frame_rejection_closes_connection() {
    let (base_url, handle, registry) = start_test_router().await;
    let (mut ws, _) = connect_peer(&base_url, "malformed-peer", PeerClass::Service).await;
    let _ = read_binary_frame(&mut ws).await;

    ws.send(Message::Binary(Bytes::from_static(&[0x00, 0x01, 0x02])))
        .await
        .unwrap();

    wait_for_peer_absent(&registry, "malformed-peer").await;
    assert!(!registry.snapshot_peer_exists("malformed-peer").await);
    handle.abort();
}

#[derive(serde::Serialize)]
struct BrokenRegisterEnvelope {
    v: u32,
    kind: String,
    msg_id: String,
    from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    ts: f64,
}

#[tokio::test]
async fn register_envelope_validation_rejects_missing_required_fields() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut ws, _) = connect_async(&base_url).await.unwrap();

    let envelope = BrokenRegisterEnvelope {
        v: PROTOCOL_VERSION,
        kind: KIND_REGISTER.to_string(),
        msg_id: "broken-register".into(),
        from: "broken-peer".into(),
        to: None,
        ts: 1_700_000_004.0,
    };
    let register = RegisterBody {
        peer_id: "broken-peer".into(),
        class: PeerClass::Service,
    };
    let envelope_bytes = rmp_serde::to_vec_named(&envelope).unwrap();
    let body_bytes = encode_register_body(&register).unwrap();
    let mut frame = Vec::with_capacity(4 + envelope_bytes.len() + body_bytes.len());
    frame.extend_from_slice(&(envelope_bytes.len() as u32).to_be_bytes());
    frame.extend_from_slice(&envelope_bytes);
    frame.extend_from_slice(&body_bytes);
    ws.send(Message::Binary(Bytes::from(frame))).await.unwrap();

    expect_connection_closed(&mut ws).await;
    handle.abort();
}

#[tokio::test]
async fn websocket_close_evicts_live_registry_entry_immediately() {
    let (base_url, handle, registry) = start_test_router().await;
    let (mut ws, _) = connect_peer(&base_url, "close-me", PeerClass::Service).await;
    let _ = read_binary_frame(&mut ws).await;
    ws.close(None).await.unwrap();
    drop(ws);
    wait_for_peer_absent(&registry, "close-me").await;
    assert!(!registry.snapshot_peer_exists("close-me").await);
    handle.abort();
}

#[tokio::test]
async fn re_register_after_disconnect_succeeds() {
    let (base_url, handle, registry) = start_test_router().await;
    let (mut ws, _) = connect_peer(&base_url, "rejoin", PeerClass::Service).await;
    let _ = read_binary_frame(&mut ws).await;
    let disconnect = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_DISCONNECT.to_string(),
        msg_id: "disconnect-1".into(),
        from: "rejoin".into(),
        to: ROUTER_PEER_ID.to_string(),
        ts: 1_700_000_005.0,
    };
    let frame = encode_frame(&disconnect, &[0x80]).unwrap();
    ws.send(Message::Binary(Bytes::from(frame))).await.unwrap();
    let _ = read_binary_frame(&mut ws).await;
    ws.close(None).await.unwrap();
    drop(ws);

    wait_for_peer_absent(&registry, "rejoin").await;
    assert!(!registry.snapshot_peer_exists("rejoin").await);

    let (mut ws2, _) = connect_peer(&base_url, "rejoin", PeerClass::Service).await;
    let ack = read_binary_frame(&mut ws2).await;
    assert_eq!(ack.envelope.kind, KIND_RESPONSE);
    handle.abort();
}

#[tokio::test]
async fn non_register_first_message_gets_router_error() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut ws, _) = connect_async(&base_url).await.unwrap();
    let envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_CALL.to_string(),
        msg_id: "first-call".into(),
        from: "peer-first".into(),
        to: ROUTER_PEER_ID.to_string(),
        ts: 1_700_000_010.0,
    };
    let frame = encode_frame(&envelope, &[0x80]).unwrap();
    ws.send(Message::Binary(Bytes::from(frame))).await.unwrap();

    let reply = timeout(Duration::from_secs(2), ws.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Binary(bytes) = reply else {
        panic!("expected binary error frame");
    };
    let decoded = decode_frame(bytes.as_ref()).unwrap();
    assert_eq!(decoded.envelope.kind, KIND_ERROR);
    let error_body = decode_error_body(&decoded.body_bytes).unwrap();
    assert_eq!(error_body.code, "register.required");
    handle.abort();
}

#[tokio::test]
async fn call_from_service_peer_gets_invalid_source_class_error() {
    let (base_url, handle, _) = start_test_router().await;
    let (mut service_ws, _) = connect_peer(&base_url, "service-src", PeerClass::Service).await;
    let _ = read_binary_frame(&mut service_ws).await;

    let envelope = Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_CALL.to_string(),
        msg_id: "bad-call".into(),
        from: "service-src".into(),
        to: "service-target".into(),
        ts: 1_700_000_011.0,
    };
    let frame = encode_frame(&envelope, &[0x80]).unwrap();
    service_ws
        .send(Message::Binary(Bytes::from(frame)))
        .await
        .unwrap();

    let reply = read_binary_frame(&mut service_ws).await;
    assert_eq!(reply.envelope.kind, KIND_ERROR);
    let error_body = decode_error_body(&reply.body_bytes).unwrap();
    assert_eq!(error_body.code, "invalid_source_class");
    handle.abort();
}
