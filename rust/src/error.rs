use thiserror::Error;

#[derive(Error, Debug)]
pub enum MultifrostError {
    #[error("Failed to serialize message: {0}")]
    SerializeError(#[from] rmp_serde::encode::Error),

    #[error("Failed to deserialize message: {0}")]
    DeserializeError(#[from] rmp_serde::decode::Error),

    #[error("ZMQ error: {0}")]
    ZmqError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Remote call error: {0}")]
    RemoteCallError(String),

    #[error("Timeout waiting for response")]
    TimeoutError,

    #[error("Worker not running")]
    NotRunningError,

    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Service already running: {0}")]
    ServiceAlreadyRunning(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, MultifrostError>;
