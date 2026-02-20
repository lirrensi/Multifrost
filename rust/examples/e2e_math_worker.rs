//! E2E Test Worker - Rust implementation
//! This worker provides various methods for cross-language testing.
//! Build: cargo build --example e2e_math_worker --release
//! Binary will be at: target/release/examples/e2e_math_worker.exe

use async_trait::async_trait;
use multifrost::{run_worker, ChildWorker, ChildWorkerContext, MultifrostError, Result};
use serde_json::Value;
use std::env;

struct MathWorker;

#[async_trait]
impl ChildWorker for MathWorker {
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

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let ctx = if args.contains(&"--service".to_string()) {
        // Service mode - register with service registry
        ChildWorkerContext::new().with_service_id("math-service")
    } else {
        // Spawn mode - parent will provide port via env
        ChildWorkerContext::new()
    };

    run_worker(MathWorker, ctx).await;
}
