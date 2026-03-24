//! E2E test worker for the v5 router flow.

use async_trait::async_trait;
use multifrost::{run_service, MultifrostError, Result, ServiceContext, ServiceWorker};
use serde_json::Value;
use std::env;

struct MathWorker;

#[async_trait]
impl ServiceWorker for MathWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        match function {
            "add" => {
                let a = args.get(0).and_then(|v| v.as_i64()).ok_or_else(|| {
                    MultifrostError::InvalidMessage("arg[0] must be an integer".into())
                })?;
                let b = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
                    MultifrostError::InvalidMessage("arg[1] must be an integer".into())
                })?;
                Ok(serde_json::json!(a + b))
            }
            "multiply" => {
                let a = args.get(0).and_then(|v| v.as_i64()).ok_or_else(|| {
                    MultifrostError::InvalidMessage("arg[0] must be an integer".into())
                })?;
                let b = args.get(1).and_then(|v| v.as_i64()).ok_or_else(|| {
                    MultifrostError::InvalidMessage("arg[1] must be an integer".into())
                })?;
                Ok(serde_json::json!(a * b))
            }
            "factorial" => {
                let n = args.get(0).and_then(|v| v.as_u64()).ok_or_else(|| {
                    MultifrostError::InvalidMessage("arg[0] must be a non-negative integer".into())
                })?;
                if n > 20 {
                    return Err(MultifrostError::InvalidMessage("input too large".into()));
                }
                let result: u64 = (1..=n).product();
                Ok(serde_json::json!(result))
            }
            "fibonacci" => {
                let n = args.get(0).and_then(|v| v.as_u64()).ok_or_else(|| {
                    MultifrostError::InvalidMessage("arg[0] must be a non-negative integer".into())
                })?;
                let result = fibonacci(n);
                Ok(serde_json::json!(result))
            }
            "echo" => {
                let value = args.get(0).cloned().unwrap_or(Value::Null);
                Ok(value)
            }
            "get_info" => Ok(serde_json::json!({
                "language": "rust",
                "pid": std::process::id()
            })),
            "throw_error" => {
                let message = args
                    .get(0)
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                Err(MultifrostError::RemoteCallError(message.to_string()))
            }
            "greet" => {
                let name = args.get(0).and_then(|v| v.as_str()).unwrap_or("World");
                Ok(serde_json::json!(format!("Hello, {}!", name)))
            }
            _ => Err(MultifrostError::FunctionNotFound(function.to_string())),
        }
    }
}

fn fibonacci(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    if n == 1 {
        return 1;
    }

    let (mut a, mut b) = (0u64, 1u64);
    for _ in 2..=n {
        let temp = a.wrapping_add(b);
        a = b;
        b = temp;
    }
    b
}

fn resolve_peer_id() -> Option<String> {
    if let Ok(peer_id) = env::var("MULTIFROST_PEER_ID") {
        let trimmed = peer_id.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let args: Vec<String> = env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--service-id") {
        if let Some(peer_id) = args.get(index + 1) {
            let trimmed = peer_id.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    if args.iter().any(|arg| arg == "--service") {
        return Some("math-service".to_string());
    }

    None
}

#[tokio::main]
async fn main() {
    let ctx = match resolve_peer_id() {
        Some(peer_id) => ServiceContext::new().with_service_id(&peer_id),
        None => ServiceContext::new(),
    };

    run_service(MathWorker, ctx).await;
}
