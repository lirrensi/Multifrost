# Multifrost Rust Quick Examples

## Install

```bash
cd rust
cargo build
```

## Caller Example

```rust
use multifrost::{call, connect};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = connect("math-service", 5_000).await?;
    let handle = connection.handle();
    handle.start().await?;

    let sum: i64 = call!(handle, add(10, 20)).await?;
    println!("10 + 20 = {sum}");

    let product: i64 = handle.call("multiply", vec![serde_json::json!(7), serde_json::json!(8)]).await?;
    println!("7 * 8 = {product}");

    handle.stop().await;
    Ok(())
}
```

## Service Example

```rust
use async_trait::async_trait;
use multifrost::{run_service, Result, ServiceContext, ServiceWorker};
use serde_json::Value;

struct MathWorker;

#[async_trait]
impl ServiceWorker for MathWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        match function {
            "add" => {
                let a = args[0].as_i64().unwrap();
                let b = args[1].as_i64().unwrap();
                Ok(serde_json::json!(a + b))
            }
            _ => Err(multifrost::MultifrostError::FunctionNotFound(function.to_string())),
        }
    }
}

#[tokio::main]
async fn main() {
    run_service(MathWorker, ServiceContext::new().with_service_id("math-service")).await;
}
```

## Spawn Then Connect

```rust
use multifrost::{connect, spawn, call};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service_bin = "./target/debug/examples/math_worker";
    let process = spawn(service_bin, service_bin).await?;

    let connection = connect("math-service", 10_000).await?;
    let handle = connection.handle();
    handle.start().await?;

    let value: i64 = call!(handle, add(2, 3)).await?;
    println!("2 + 3 = {value}");

    handle.stop().await;
    let mut process = process;
    process.stop()?;
    Ok(())
}
```

## Router Bootstrap Notes

- `connect(...)` and `run_service(...)` both bootstrap the router if it is not
  already reachable
- the router port defaults to `9981`
- `MULTIFROST_ROUTER_PORT` overrides the port
- `MULTIFROST_ENTRYPOINT_PATH` helps the service side resolve its default
  `peer_id`
