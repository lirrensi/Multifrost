use std::time::Duration;

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use multifrost_router::protocol::{
    decode_frame, encode_frame, encode_query_body, encode_register_body, Envelope, FrameParts,
    PeerClass, QueryBody, RegisterBody, KIND_CALL, KIND_DISCONNECT, KIND_ERROR, KIND_QUERY,
    KIND_REGISTER, KIND_RESPONSE, PROTOCOL_VERSION, ROUTER_PEER_ID,
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
    handle.abort();
}

#[tokio::test]
async fn websocket_close_evicts_live_registry_entry_immediately() {
    let (base_url, handle, registry) = start_test_router().await;
    let (mut ws, _) = connect_peer(&base_url, "close-me", PeerClass::Service).await;
    let _ = read_binary_frame(&mut ws).await;
    ws.close(None).await.unwrap();
    drop(ws);
    tokio::time::sleep(Duration::from_millis(200)).await;
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

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!registry.snapshot_peer_exists("rejoin").await);

    let (mut ws2, _) = connect_peer(&base_url, "rejoin", PeerClass::Service).await;
    let ack = read_binary_frame(&mut ws2).await;
    assert_eq!(ack.envelope.kind, KIND_RESPONSE);
    handle.abort();
}
