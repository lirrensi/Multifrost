//! Math worker example
//! 
//! Spawn mode (parent spawns this): cargo run --example math_worker
//! Service mode (standalone):       cargo run --example math_worker -- --service

use multifrost::{ChildWorker, ChildWorkerContext, MultifrostError, Result, run_worker};
use async_trait::async_trait;
use serde_json::Value;
use std::env;

struct MathWorker;

#[async_trait]
impl ChildWorker for MathWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
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
            "factorial" => {
                let n = args.get(0).and_then(|v| v.as_u64()).unwrap_or(0);
                let result: u64 = (1..=n).product();
                Ok(serde_json::json!(result))
            }
            "fibonacci" => {
                let n = args.get(0).and_then(|v| v.as_u64()).unwrap_or(0);
                let result = fibonacci(n);
                Ok(serde_json::json!(result))
            }
            _ => Err(MultifrostError::FunctionNotFound(function.to_string()))
        }
    }
}

fn fibonacci(n: u64) -> u64 {
    if n == 0 { return 0; }
    if n == 1 { return 1; }
    
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
