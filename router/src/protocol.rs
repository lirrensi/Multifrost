use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

use crate::error::{Result, RouterError};

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
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub kind: String,
    pub stack: Option<String>,
    pub details: Option<WireValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameParts {
    pub envelope: Envelope,
    pub body_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WireValue {
    Nil,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Float(f64),
    Text(String),
    Binary(Vec<u8>),
    Array(Vec<WireValue>),
    Map(Vec<(String, WireValue)>),
}

pub fn validate_protocol_key(value: &str) -> Result<()> {
    if value == PROTOCOL_KEY {
        Ok(())
    } else {
        Err(RouterError::InvalidProtocolKey(value.to_string()))
    }
}

pub fn ensure_binary_ws_message(message: Message) -> Result<Vec<u8>> {
    match message {
        Message::Binary(bytes) => Ok(bytes.to_vec()),
        _ => Err(RouterError::NonBinaryMessage),
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
        return Err(RouterError::MalformedFrame(
            "frame too short for envelope length".into(),
        ));
    }

    let envelope_len = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as usize;
    if bytes.len() < 4 + envelope_len {
        return Err(RouterError::MalformedFrame(
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
    rmp_serde::to_vec_named(envelope).map_err(|err| RouterError::Other(err.to_string()))
}

pub fn decode_envelope(bytes: &[u8]) -> Result<Envelope> {
    let envelope: Envelope =
        rmp_serde::from_slice(bytes).map_err(|err| RouterError::MalformedFrame(err.to_string()))?;
    if envelope.v != PROTOCOL_VERSION {
        return Err(RouterError::InvalidProtocolKey(format!("v={}", envelope.v)));
    }
    Ok(envelope)
}

pub fn encode_register_body(body: &RegisterBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(|err| RouterError::Other(err.to_string()))
}

pub fn decode_register_body(bytes: &[u8]) -> Result<RegisterBody> {
    rmp_serde::from_slice(bytes).map_err(|err| RouterError::MalformedRegisterBody(err.to_string()))
}

pub fn encode_register_ack_body(body: &RegisterAckBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(|err| RouterError::Other(err.to_string()))
}

pub fn encode_query_body(body: &QueryBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(|err| RouterError::Other(err.to_string()))
}

pub fn decode_query_body(bytes: &[u8]) -> Result<QueryBody> {
    rmp_serde::from_slice(bytes).map_err(|err| RouterError::MalformedQueryBody(err.to_string()))
}

pub fn encode_query_exists_response(
    peer_id: &str,
    exists: bool,
    class: Option<PeerClass>,
    connected: bool,
) -> Result<Vec<u8>> {
    let body = QueryExistsResponseBody {
        peer_id: peer_id.to_string(),
        exists,
        class,
        connected,
    };
    rmp_serde::to_vec_named(&body).map_err(|err| RouterError::Other(err.to_string()))
}

pub fn encode_query_get_response(
    peer_id: &str,
    exists: bool,
    class: Option<PeerClass>,
    connected: bool,
) -> Result<Vec<u8>> {
    let body = QueryGetResponseBody {
        peer_id: peer_id.to_string(),
        exists,
        class,
        connected,
    };
    rmp_serde::to_vec_named(&body).map_err(|err| RouterError::Other(err.to_string()))
}

pub fn encode_error_body(body: &ErrorBody) -> Result<Vec<u8>> {
    rmp_serde::to_vec_named(body).map_err(|err| RouterError::Other(err.to_string()))
}

pub fn decode_error_body(bytes: &[u8]) -> Result<ErrorBody> {
    rmp_serde::from_slice(bytes).map_err(|err| RouterError::MalformedFrame(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn register_body_round_trip() {
        let body = RegisterBody {
            peer_id: "peer".into(),
            class: PeerClass::Service,
        };
        let bytes = encode_register_body(&body).unwrap();
        let decoded = decode_register_body(&bytes).unwrap();
        assert_eq!(decoded, body);
    }
}
