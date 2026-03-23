use thiserror::Error;

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("invalid protocol key: {0}")]
    InvalidProtocolKey(String),

    #[error("invalid port: {0}")]
    InvalidPort(String),

    #[error("non-binary websocket message")]
    NonBinaryMessage,

    #[error("malformed frame: {0}")]
    MalformedFrame(String),

    #[error("malformed register body: {0}")]
    MalformedRegisterBody(String),

    #[error("malformed query body: {0}")]
    MalformedQueryBody(String),

    #[error("duplicate live peer_id: {0}")]
    DuplicatePeerId(String),

    #[error("peer not found: {0}")]
    PeerNotFound(String),

    #[error("invalid target class: {0}")]
    InvalidTargetClass(String),

    #[error("invalid source class: {0}")]
    InvalidSourceClass(String),

    #[error("connection closed")]
    ConnectionClosed,

    #[error("router error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, RouterError>;
