//! Math worker example
//!
//! Spawn mode (parent spawns this): cargo run --example math_worker
//! Service mode (standalone):       cargo run --example math_worker -- --service
//! Named service mode:              cargo run --example math_worker -- --service-id math-service-a

use async_trait::async_trait;
use multifrost::{run_service, MultifrostError, Result, ServiceContext, ServiceWorker};
use serde_json::Value;
use std::env;

struct MathWorker {
    label: String,
}

#[async_trait]
impl ServiceWorker for MathWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        println!("[{}] handling {}", self.label, function);
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

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    let service_id = flag_value(&args, "--service-id").or_else(|| {
        if args.iter().any(|arg| arg == "--service") {
            Some("math-service".to_string())
        } else {
            None
        }
    });

    let label = service_id
        .clone()
        .unwrap_or_else(|| "spawn-mode".to_string());
    let ctx = if let Some(service_id) = service_id {
        println!("Math worker starting in service mode as {service_id}");
        ServiceContext::new().with_service_id(&service_id)
    } else {
        println!("Math worker starting in spawn mode");
        ServiceContext::new()
    };

    run_service(MathWorker { label }, ctx).await;
}
