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

    #[error("Circuit breaker is open after {0} consecutive failures")]
    CircuitOpenError(usize),

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

    #[error("msgpack contains NaN/Infinity values")]
    MsgpackNaNInfinity,

    #[error("msgpack size limit exceeded: {0}")]
    MsgpackSizeLimit(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

pub type Result<T> = std::result::Result<T, MultifrostError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_zmq_error() {
        let err = MultifrostError::ZmqError("Connection failed".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("ZMQ error"));
        assert!(msg.contains("Connection failed"));
    }

    #[test]
    fn test_error_display_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let err = MultifrostError::IoError(io_err);
        let msg = format!("{}", err);
        assert!(msg.contains("IO error"));
        assert!(msg.contains("File not found"));
    }

    #[test]
    fn test_error_display_remote_call_error() {
        let err = MultifrostError::RemoteCallError("Function failed".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Remote call error"));
        assert!(msg.contains("Function failed"));
    }

    #[test]
    fn test_error_display_circuit_open_error() {
        let err = MultifrostError::CircuitOpenError(5);
        let msg = format!("{}", err);
        assert!(msg.contains("Circuit breaker is open"));
        assert!(msg.contains("5"));
    }

    #[test]
    fn test_error_display_timeout_error() {
        let err = MultifrostError::TimeoutError;
        let msg = format!("{}", err);
        assert!(msg.contains("Timeout waiting for response"));
    }

    #[test]
    fn test_error_display_not_running_error() {
        let err = MultifrostError::NotRunningError;
        let msg = format!("{}", err);
        assert!(msg.contains("Worker not running"));
    }

    #[test]
    fn test_error_display_service_not_found() {
        let err = MultifrostError::ServiceNotFound("my-service".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Service not found"));
        assert!(msg.contains("my-service"));
    }

    #[test]
    fn test_error_display_service_already_running() {
        let err = MultifrostError::ServiceAlreadyRunning("my-service".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Service already running"));
        assert!(msg.contains("my-service"));
    }

    #[test]
    fn test_error_display_invalid_message() {
        let err = MultifrostError::InvalidMessage("Malformed data".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Invalid message"));
        assert!(msg.contains("Malformed data"));
    }

    #[test]
    fn test_error_display_function_not_found() {
        let err = MultifrostError::FunctionNotFound("unknown_func".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("Function not found"));
        assert!(msg.contains("unknown_func"));
    }

    #[test]
    fn test_error_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access denied");
        let err: MultifrostError = io_err.into();
        assert!(matches!(err, MultifrostError::IoError(_)));
    }

    #[test]
    fn test_error_debug() {
        let err = MultifrostError::TimeoutError;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("TimeoutError"));
    }

    #[test]
    fn test_result_type_alias() {
        let ok_result: Result<i32> = Ok(42);
        assert!(ok_result.is_ok());

        let err_result: Result<i32> = Err(MultifrostError::TimeoutError);
        assert!(err_result.is_err());
    }

    #[test]
    fn test_error_clone() {
        let err1 = MultifrostError::RemoteCallError("test".to_string());
        // Note: MultifrostError doesn't implement Clone due to underlying error types
        // This test just verifies the error can be formatted
        let msg = format!("{}", err1);
        assert!(msg.contains("test"));
    }

    #[test]
    fn test_circuit_open_error_with_different_counts() {
        let err1 = MultifrostError::CircuitOpenError(3);
        let err2 = MultifrostError::CircuitOpenError(10);
        assert_ne!(format!("{}", err1), format!("{}", err2));
    }
}
