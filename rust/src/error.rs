use thiserror::Error;

#[derive(Error, Debug)]
pub enum MultifrostError {
    #[error("Failed to serialize message: {0}")]
    SerializeError(#[from] rmp_serde::encode::Error),

    #[error("Failed to deserialize message: {0}")]
    DeserializeError(#[from] rmp_serde::decode::Error),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Timeout waiting for response")]
    TimeoutError,

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Malformed frame: {0}")]
    MalformedFrame(String),

    #[error("Bootstrap failure: {0}")]
    BootstrapFailure(String),

    #[error("Router unavailable: {0}")]
    RouterUnavailable(String),

    #[error("Register rejected: {0}")]
    RegisterRejected(String),

    #[error("Invalid target class: {0}")]
    InvalidTargetClass(String),

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Protocol key mismatch: {0}")]
    InvalidProtocolKey(String),

    #[error("Remote call error: {0}")]
    RemoteCallError(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

pub type Result<T> = std::result::Result<T, MultifrostError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_router_unavailable() {
        let err = MultifrostError::RouterUnavailable("ws://127.0.0.1:9981".into());
        assert!(format!("{err}").contains("Router unavailable"));
    }

    #[test]
    fn displays_register_rejected() {
        let err = MultifrostError::RegisterRejected("duplicate peer_id".into());
        assert!(format!("{err}").contains("Register rejected"));
    }

    #[test]
    fn displays_invalid_target_class() {
        let err = MultifrostError::InvalidTargetClass("caller".into());
        assert!(format!("{err}").contains("Invalid target class"));
    }
}
