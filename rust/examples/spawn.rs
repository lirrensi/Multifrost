//! Spawn example - spawns and calls the math worker
//!
//! Run with: cargo run --example spawn

use multifrost::{ParentWorker, ParentWorkerBuilder, call, LifecycleEvent};
use std::env;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Spawning math worker...");

    // Get the current directory and build the path to the math_worker binary
    let current_dir = env::current_dir().expect("Failed to get current dir");
    let worker_path = current_dir
        .join("target")
        .join("debug")
        .join("examples")
        .join("math_worker.exe");

    // Method 1: Simple spawn
    // let mut worker = ParentWorker::spawn("", worker_path.to_str().unwrap()).await?;

    // Method 2: Spawn with builder pattern (recommended)
    let mut worker = ParentWorkerBuilder::spawn("", worker_path.to_str().unwrap())
        .auto_restart(false)
        .default_timeout(Duration::from_secs(30))
        .stdout_handler(|output| println!("[CUSTOM STDOUT]: {}", output))
        .build()
        .await?;

    // Subscribe to lifecycle events
    let mut event_stream = worker.subscribe();
    tokio::spawn(async move {
        while let Some(event) = event_stream.recv().await {
            println!("Event: {:?}", event);
        }
    });

    // start() waits for child to be ready before returning
    worker.start().await?;
    println!("Worker started!\n");

    println!("Calling remote functions...\n");

    // Using the ergonomic call! macro (recommended)
    let result: i64 = worker.call!(add(10, 20)).await?;
    println!("add(10, 20) = {}", result);

    let result: i64 = worker.call!(multiply(7, 8)).await?;
    println!("multiply(7, 8) = {}", result);

    // Using traditional API for comparison
    use serde_json::json;
    let result: u64 = worker.call("factorial", vec![json!(5)]).await?;
    println!("factorial(5) = {}", result);

    let result: u64 = worker.call("fibonacci", vec![json!(10)]).await?;
    println!("fibonacci(10) = {}", result);

    println!("\nDone! Stopping worker...");
    worker.stop().await;

    Ok(())
}
