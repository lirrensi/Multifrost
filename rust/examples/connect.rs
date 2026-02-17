//! Connect example - connects to an existing service
//!
//! Run the math_worker first in service mode, then run this example.
//!
//! Service mode: cargo run --example math_worker -- --service
//! Connect mode: cargo run --example connect

use multifrost::{ParentWorker, ParentWorkerBuilder, ConnectOptions, call, LifecycleEvent};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Connecting to math service...");

    // Method 1: Simple connect with timeout
    // let handle = ParentWorker::connect("math-service", 5000).await?.handle();

    // Method 2: Connect with options (consistent with spawn API)
    let options = ConnectOptions {
        default_timeout: Some(10_000), // 10 second timeout
        heartbeat_interval: 5.0,
        heartbeat_timeout: 3.0,
        heartbeat_max_misses: 3,
        enable_metrics: true,
        stdout_handler: None,
        stderr_handler: None,
    };

    let worker = ParentWorker::connect_with_options("math-service", options).await?;

    // Method 3: Using builder pattern
    // let worker = ParentWorkerBuilder::connect("math-service")
    //     .default_timeout(Duration::from_secs(10))
    //     .build()
    //     .await?;

    // Get handle - this consumes the worker
    let mut handle = worker.handle();

    // Subscribe to lifecycle events
    let mut event_stream = handle.subscribe();
    tokio::spawn(async move {
        while let Some(event) = event_stream.recv().await {
            println!("Event: {:?}", event);
        }
    });

    handle.start().await?;
    println!("Connected to service!\n");

    println!("Calling remote functions...\n");

    // Using the ergonomic call! macro
    let result: i64 = call!(handle, add(10, 20)).await?;
    println!("add(10, 20) = {}", result);

    let result: i64 = call!(handle, multiply(7, 8)).await?;
    println!("multiply(7, 8) = {}", result);

    // Using traditional API for comparison
    use serde_json::json;
    let result: i64 = handle.call("factorial", vec![json!(5)]).await?;
    println!("factorial(5) = {}", result);

    println!("\nDone! Stopping connection...");
    handle.stop().await;

    Ok(())
}