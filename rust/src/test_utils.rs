//! Test utilities for Multifrost library.
//!
//! Provides common helpers for unit and integration tests.

use crate::message::Message;
use crate::metrics::Metrics;
use crate::logging::{StructuredLogger, LogLevel, LogEvent};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Test worker implementation for testing ChildWorker trait.
#[derive(Clone)]
pub struct TestWorker {
    pub should_fail: Arc<Mutex<bool>>,
}

impl TestWorker {
    pub fn new() -> Self {
        Self {
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_fail(&self, fail: bool) {
        *self.should_fail.lock().unwrap() = fail;
    }
}

impl Default for TestWorker {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl crate::child::ChildWorker for TestWorker {
    async fn handle_call(&self, function: &str, args: Vec<serde_json::Value>) -> crate::error::Result<serde_json::Value> {
        if *self.should_fail.lock().unwrap() {
            return Err(crate::error::MultifrostError::RemoteCallError("Test worker set to fail".to_string()));
        }

        match function {
            "add" => {
                let a = args.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
                let b = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                Ok(serde_json::json!(a + b))
            }
            "multiply" => {
                let a = args.get(0).and_then(|v| v.as_i64()).unwrap_or(0);
                let b = args.get(1).and_then(|v| v.as_i64()).unwrap_or(0);
                Ok(serde_json::json!(a * b))
            }
            "echo" => {
                Ok(args.get(0).cloned().unwrap_or(serde_json::json!(null)))
            }
            "fail" => {
                Err(crate::error::MultifrostError::RemoteCallError("Intentional test failure".to_string()))
            }
            "sleep" => {
                let ms = args.get(0).and_then(|v| v.as_u64()).unwrap_or(100);
                tokio::time::sleep(Duration::from_millis(ms)).await;
                Ok(serde_json::json!("slept"))
            }
            _ => {
                Err(crate::error::MultifrostError::FunctionNotFound(function.to_string()))
            }
        }
    }
}

/// Helper to create a test message.
pub fn create_test_message(msg_type: crate::message::MessageType) -> Message {
    Message::new(msg_type)
}

/// Helper to create a test call message.
pub fn create_test_call(function: &str, args: Vec<serde_json::Value>) -> Message {
    Message::create_call(function, args)
}

/// Helper to create a test response message.
pub fn create_test_response(result: serde_json::Value, msg_id: &str) -> Message {
    Message::create_response(result, msg_id)
}

/// Helper to create a test error message.
pub fn create_test_error(error: &str, msg_id: &str) -> Message {
    Message::create_error(error, msg_id)
}

/// Helper to create a test heartbeat message.
pub fn create_test_heartbeat(msg_id: Option<String>, timestamp: Option<f64>) -> Message {
    Message::create_heartbeat(msg_id, timestamp)
}

/// Helper to create a test metrics instance.
pub fn create_test_metrics() -> Metrics {
    Metrics::new()
}

/// Helper to create a test logger with a capturing handler.
pub fn create_test_logger() -> (StructuredLogger, Arc<Mutex<Vec<String>>>) {
    let logs = Arc::new(Mutex::new(Vec::new()));
    let logs_clone = Arc::clone(&logs);

    let handler = Arc::new(move |entry: &crate::logging::LogEntry| {
        logs_clone.lock().unwrap().push(entry.to_json());
    });

    let logger = StructuredLogger::new(
        Some(handler),
        LogLevel::Debug,
        Some("test-worker".to_string()),
        None,
    );

    (logger, logs)
}

/// Helper to wait for a condition with timeout.
pub async fn wait_for_condition<F, Fut>(condition: F, timeout_ms: u64) -> bool
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    while start.elapsed() < timeout {
        if condition().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    false
}

/// Helper to get a free port for testing.
pub fn get_test_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr().map(|a| a.port()))
        .unwrap_or(5555)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::child::ChildWorker;

    #[tokio::test]
    async fn test_test_worker_add() {
        let worker = TestWorker::new();
        let result = worker.handle_call("add", vec![json!(5), json!(3)]).await.unwrap();
        assert_eq!(result, json!(8));
    }

    #[tokio::test]
    async fn test_test_worker_multiply() {
        let worker = TestWorker::new();
        let result = worker.handle_call("multiply", vec![json!(4), json!(7)]).await.unwrap();
        assert_eq!(result, json!(28));
    }

    #[tokio::test]
    async fn test_test_worker_echo() {
        let worker = TestWorker::new();
        let result = worker.handle_call("echo", vec![json!("hello")]).await.unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[tokio::test]
    async fn test_test_worker_fail() {
        let worker = TestWorker::new();
        let result: crate::error::Result<serde_json::Value> = worker.handle_call("fail", vec![]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_test_worker_not_found() {
        let worker = TestWorker::new();
        let result: crate::error::Result<serde_json::Value> = worker.handle_call("unknown", vec![]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_test_worker_set_fail() {
        let worker = TestWorker::new();
        worker.set_fail(true);
        let result: crate::error::Result<serde_json::Value> = worker.handle_call("add", vec![json!(1), json!(2)]).await;
        assert!(result.is_err());
    }
}
