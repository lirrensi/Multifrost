//! Spawn example - starts a service peer and talks to it through the router.

use multifrost::{call, connect, spawn};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Spawning math worker...");

    // Get the current directory and build the path to the math_worker binary
    let current_dir = env::current_dir().expect("Failed to get current dir");
    let worker_path = current_dir
        .join("target")
        .join("debug")
        .join("examples")
        .join(format!("math_worker{}", std::env::consts::EXE_SUFFIX));

    let service_process = spawn(
        worker_path.to_str().expect("worker path to be valid utf-8"),
        worker_path.to_str().expect("worker path to be valid utf-8"),
    )
    .await?;
    let connection = connect(
        worker_path.to_str().expect("worker path to be valid utf-8"),
        10_000,
    )
    .await?
    .with_service_process(service_process);
    let handle = connection.handle();
    handle.start().await?;
    println!("Worker started!\n");

    println!("Calling remote functions...\n");

    let result: i64 = call!(handle, add(10, 20)).await?;
    println!("add(10, 20) = {}", result);

    let result: i64 = call!(handle, multiply(7, 8)).await?;
    println!("multiply(7, 8) = {}", result);

    use serde_json::json;
    let result: u64 = handle.call("factorial", vec![json!(5)]).await?;
    println!("factorial(5) = {}", result);

    let result: u64 = handle.call("fibonacci", vec![json!(10)]).await?;
    println!("fibonacci(10) = {}", result);

    println!("\nDone! Stopping worker...");
    handle.stop().await;

    Ok(())
}
