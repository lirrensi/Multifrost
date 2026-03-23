use crate::error::{MultifrostError, Result};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

pub const PROTOCOL_KEY: &str = "multifrost_ipc_v5";
pub const PROTOCOL_VERSION: u32 = 5;
pub const ROUTER_PEER_ID: &str = "router";

pub const KIND_REGISTER: &str = "register";
pub const KIND_QUERY: &str = "query";
pub const KIND_CALL: &str = "call";
pub const KIND_RESPONSE: &str = "response";
pub const KIND_ERROR: &str = "error";
pub const KIND_HEARTBEAT: &str = "heartbeat";
pub const KIND_DISCONNECT: &str = "disconnect";

pub const QUERY_PEER_EXISTS: &str = "peer.exists";
pub const QUERY_PEER_GET: &str = "peer.get";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub v: u32,
    pub kind: String,
    pub msg_id: String,
    pub from: String,
    pub to: String,
    pub ts: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PeerClass {
    Service,
    Caller,
}

impl PeerClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Caller => "caller",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterBody {
    pub peer_id: String,
    pub class: PeerClass,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegisterAckBody {
    pub accepted: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "query")]
pub enum QueryBody {
    #[serde(rename = "peer.exists")]
    PeerExists { peer_id: String },
    #[serde(rename = "peer.get")]
    PeerGet { peer_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryExistsResponseBody {
    pub peer_id: String,
    pub exists: bool,
    pub class: Option<PeerClass>,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueryGetResponseBody {
    pub peer_id: String,
    pub exists: bool,
    pub class: Option<PeerClass>,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallBody {
    pub function: String,
    #[serde(default)]
    pub args: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseBody {
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<WireValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WireValue {
    Nil,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Text(String),
    #[serde(with = "serde_bytes")]
    Binary(Vec<u8>),
    Array(Vec<WireValue>),
    Map(Vec<(String, WireValue)>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameParts {
    pub envelope: Envelope,
    pub body_bytes: Vec<u8>,
}

pub fn now_ts() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn new_msg_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn validate_protocol_key(value: &str) -> Result<()> {
    if value == PROTOCOL_KEY {
        Ok(())
    } else {
        Err(MultifrostError::InvalidProtocolKey(value.to_string()))
    }
}

pub fn ensure_binary_ws_message(message: Message) -> Result<Vec<u8>> {
    match message {
        Message::Binary(bytes) => Ok(bytes.to_vec()),
        other => Err(MultifrostError::MalformedFrame(format!(
            "expected binary websocket message, got {other:?}"
        ))),
    }
}

pub fn encode_frame(envelope: &Envelope, body_bytes: &[u8]) -> Result<Vec<u8>> {
    let envelope_bytes = encode_envelope(envelope)?;
    let mut out = Vec::with_capacity(4 + envelope_bytes.len() + body_bytes.len());
    out.extend_from_slice(&(envelope_bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(&envelope_bytes);
    out.extend_from_slice(body_bytes);
    Ok(out)
}

pub fn decode_frame(bytes: &[u8]) -> Result<FrameParts> {
    if bytes.len() < 4 {
        return Err(MultifrostError::MalformedFrame(
            "frame too short for envelope length".into(),
        ));
    }

    let envelope_len = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
    if bytes.len() < 4 + envelope_len {
        return Err(MultifrostError::MalformedFrame(
            "frame shorter than declared envelope length".into(),
        ));
    }

    let envelope = decode_envelope(&bytes[4..4 + envelope_len])?;
    let body_bytes = bytes[4 + envelope_len..].to_vec();
    Ok(FrameParts {
        envelope,
        body_bytes,
    })
}

pub fn encode_envelope(envelope: &Envelope) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(envelope).map_err(Into::into)
}

pub fn decode_envelope(bytes: &[u8]) -> Result<Envelope> {
    let envelope: Envelope = rmp_serde::from_slice(bytes)
        .map_err(|err| MultifrostError::MalformedFrame(err.to_string()))?;
    if envelope.v != PROTOCOL_VERSION {
        return Err(MultifrostError::InvalidProtocolKey(format!(
            "v={}",
            envelope.v
        )));
    }
    Ok(envelope)
}

pub fn encode_register_body(body: &RegisterBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(Into::into)
}

pub fn decode_register_body(bytes: &[u8]) -> Result<RegisterBody> {
    rmp_serde::from_slice(bytes).map_err(|err| MultifrostError::MalformedFrame(err.to_string()))
}

pub fn encode_register_ack_body(body: &RegisterAckBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(Into::into)
}

pub fn decode_register_ack_body(bytes: &[u8]) -> Result<RegisterAckBody> {
    rmp_serde::from_slice(bytes).map_err(|err| MultifrostError::MalformedFrame(err.to_string()))
}

pub fn encode_query_body(body: &QueryBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(Into::into)
}

pub fn decode_query_body(bytes: &[u8]) -> Result<QueryBody> {
    rmp_serde::from_slice(bytes).map_err(|err| MultifrostError::MalformedFrame(err.to_string()))
}

pub fn encode_query_exists_response(body: &QueryExistsResponseBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(Into::into)
}

pub fn decode_query_exists_response(bytes: &[u8]) -> Result<QueryExistsResponseBody> {
    rmp_serde::from_slice(bytes).map_err(|err| MultifrostError::MalformedFrame(err.to_string()))
}

pub fn encode_query_get_response(body: &QueryGetResponseBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(Into::into)
}

pub fn decode_query_get_response(bytes: &[u8]) -> Result<QueryGetResponseBody> {
    rmp_serde::from_slice(bytes).map_err(|err| MultifrostError::MalformedFrame(err.to_string()))
}

pub fn encode_call_body(body: &CallBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(Into::into)
}

pub fn decode_call_body(bytes: &[u8]) -> Result<CallBody> {
    rmp_serde::from_slice(bytes).map_err(|err| MultifrostError::MalformedFrame(err.to_string()))
}

pub fn encode_response_body(body: &ResponseBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(Into::into)
}

pub fn decode_response_body(bytes: &[u8]) -> Result<ResponseBody> {
    rmp_serde::from_slice(bytes).map_err(|err| MultifrostError::MalformedFrame(err.to_string()))
}

pub fn encode_error_body(body: &ErrorBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(Into::into)
}

pub fn decode_error_body(bytes: &[u8]) -> Result<ErrorBody> {
    rmp_serde::from_slice(bytes).map_err(|err| MultifrostError::MalformedFrame(err.to_string()))
}

pub fn build_register_envelope(peer_id: &str, _class: PeerClass) -> Envelope {
    Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_REGISTER.to_string(),
        msg_id: new_msg_id(),
        from: peer_id.to_string(),
        to: ROUTER_PEER_ID.to_string(),
        ts: now_ts(),
    }
}

pub fn build_query_envelope(peer_id: &str, to: &str) -> Envelope {
    Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_QUERY.to_string(),
        msg_id: new_msg_id(),
        from: peer_id.to_string(),
        to: to.to_string(),
        ts: now_ts(),
    }
}

pub fn build_call_envelope(from: &str, to: &str) -> Envelope {
    Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_CALL.to_string(),
        msg_id: new_msg_id(),
        from: from.to_string(),
        to: to.to_string(),
        ts: now_ts(),
    }
}

pub fn build_response_envelope(from: &str, to: &str, msg_id: &str) -> Envelope {
    Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_RESPONSE.to_string(),
        msg_id: msg_id.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        ts: now_ts(),
    }
}

pub fn build_error_envelope(from: &str, to: &str, msg_id: &str) -> Envelope {
    Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_ERROR.to_string(),
        msg_id: msg_id.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        ts: now_ts(),
    }
}

pub fn build_disconnect_envelope(from: &str, to: &str, msg_id: &str) -> Envelope {
    Envelope {
        v: PROTOCOL_VERSION,
        kind: KIND_DISCONNECT.to_string(),
        msg_id: msg_id.to_string(),
        from: from.to_string(),
        to: to.to_string(),
        ts: now_ts(),
    }
}

pub fn binary_message(bytes: Vec<u8>) -> Message {
    Message::Binary(Bytes::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frame_round_trip_preserves_body_bytes() {
        let envelope = Envelope {
            v: PROTOCOL_VERSION,
            kind: KIND_CALL.to_string(),
            msg_id: "abc".to_string(),
            from: "caller".to_string(),
            to: "service".to_string(),
            ts: 1_700_000_000.5,
        };
        let body_bytes = vec![0x82, 0xa3, b'f', b'o', b'o', 0x01];

        let encoded = encode_frame(&envelope, &body_bytes).unwrap();
        let decoded = decode_frame(&encoded).unwrap();

        assert_eq!(decoded.envelope, envelope);
        assert_eq!(decoded.body_bytes, body_bytes);
    }

    #[test]
    fn frame_decode_rejects_short_input() {
        let err = decode_frame(&[0x00, 0x00, 0x00]).unwrap_err();
        assert!(matches!(err, MultifrostError::MalformedFrame(_)));
    }

    #[test]
    fn frame_decode_rejects_truncated_envelope() {
        let err = decode_frame(&[0x00, 0x00, 0x00, 0x10, 0x01]).unwrap_err();
        assert!(matches!(err, MultifrostError::MalformedFrame(_)));
    }

    #[test]
    fn envelope_decode_rejects_version_mismatch() {
        let envelope = Envelope {
            v: PROTOCOL_VERSION + 1,
            kind: KIND_CALL.to_string(),
            msg_id: "abc".to_string(),
            from: "caller".to_string(),
            to: "service".to_string(),
            ts: 1_700_000_000.5,
        };
        let bytes = encode_envelope(&envelope).unwrap();
        let err = decode_envelope(&bytes).unwrap_err();
        assert!(matches!(err, MultifrostError::InvalidProtocolKey(_)));
    }

    #[test]
    fn validate_protocol_key_accepts_and_rejects() {
        assert!(validate_protocol_key(PROTOCOL_KEY).is_ok());
        assert!(matches!(
            validate_protocol_key("not-the-key"),
            Err(MultifrostError::InvalidProtocolKey(_))
        ));
    }

    #[test]
    fn binary_message_wraps_bytes() {
        let message = binary_message(vec![1, 2, 3]);
        assert!(matches!(message, Message::Binary(_)));
    }

    #[test]
    fn register_ack_round_trip() {
        let body = RegisterAckBody {
            accepted: true,
            reason: None,
        };
        let bytes = encode_register_ack_body(&body).unwrap();
        let decoded = decode_register_ack_body(&bytes).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn query_response_round_trips() {
        let exists = QueryExistsResponseBody {
            peer_id: "service".into(),
            exists: true,
            class: Some(PeerClass::Service),
            connected: true,
        };
        let exists_bytes = encode_query_exists_response(&exists).unwrap();
        assert_eq!(decode_query_exists_response(&exists_bytes).unwrap(), exists);

        let get = QueryGetResponseBody {
            peer_id: "service".into(),
            exists: true,
            class: Some(PeerClass::Service),
            connected: true,
        };
        let get_bytes = encode_query_get_response(&get).unwrap();
        assert_eq!(decode_query_get_response(&get_bytes).unwrap(), get);
    }

    #[test]
    fn register_body_round_trip() {
        let body = RegisterBody {
            peer_id: "peer".into(),
            class: PeerClass::Service,
        };
        let bytes = encode_register_body(&body).unwrap();
        let decoded = decode_register_body(&bytes).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn query_body_round_trip() {
        let body = QueryBody::PeerExists {
            peer_id: "peer".into(),
        };
        let bytes = encode_query_body(&body).unwrap();
        let decoded = decode_query_body(&bytes).unwrap();
        assert_eq!(decoded, body);
    }

    #[test]
    fn call_and_response_and_error_bodies_round_trip() {
        let call = CallBody {
            function: "add".into(),
            args: vec![json!(10), json!(20)],
            namespace: Some("default".into()),
        };
        let call_bytes = encode_call_body(&call).unwrap();
        assert_eq!(decode_call_body(&call_bytes).unwrap(), call);

        let response = ResponseBody { result: json!(30) };
        let response_bytes = encode_response_body(&response).unwrap();
        assert_eq!(decode_response_body(&response_bytes).unwrap(), response);

        let error = ErrorBody {
            code: "boom".into(),
            message: "boom".into(),
            kind: "application".into(),
            stack: Some("stack".into()),
            details: Some(WireValue::Int(1)),
        };
        let error_bytes = encode_error_body(&error).unwrap();
        assert_eq!(decode_error_body(&error_bytes).unwrap(), error);
    }

    #[test]
    fn wire_value_round_trip_preserves_nested_lists() {
        let value = WireValue::Array(vec![
            WireValue::Bool(true),
            WireValue::Text("ok".into()),
            WireValue::Int(3),
        ]);
        let bytes = rmp_serde::to_vec_named(&value).unwrap();
        let decoded: WireValue = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(decoded, value);
    }

    #[test]
    fn envelope_builders_use_expected_fields() {
        let register = build_register_envelope("caller-1", PeerClass::Caller);
        assert_eq!(register.v, PROTOCOL_VERSION);
        assert_eq!(register.kind, KIND_REGISTER);
        assert_eq!(register.from, "caller-1");
        assert_eq!(register.to, ROUTER_PEER_ID);

        let query = build_query_envelope("caller-1", "router");
        assert_eq!(query.kind, KIND_QUERY);
        assert_eq!(query.to, "router");

        let call = build_call_envelope("caller-1", "service-1");
        assert_eq!(call.kind, KIND_CALL);
        assert_eq!(call.to, "service-1");

        let response = build_response_envelope("service-1", "caller-1", "msg-1");
        assert_eq!(response.kind, KIND_RESPONSE);
        assert_eq!(response.msg_id, "msg-1");

        let error = build_error_envelope("router", "caller-1", "msg-2");
        assert_eq!(error.kind, KIND_ERROR);
        assert_eq!(error.to, "caller-1");

        let disconnect = build_disconnect_envelope("caller-1", "router", "msg-3");
        assert_eq!(disconnect.kind, KIND_DISCONNECT);
        assert_eq!(disconnect.msg_id, "msg-3");
    }
}
