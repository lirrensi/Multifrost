//! Spawn example - spawns and calls the math worker
//! 
//! Run with: cargo run --example spawn

use multifrost::{ParentWorker, MultifrostError};
use std::env;

#[tokio::main]
async fn main() -> Result<(), MultifrostError> {
    println!("Spawning math worker...");
    
    // Get the current directory and build the path to the math_worker binary
    let current_dir = env::current_dir().expect("Failed to get current dir");
    let worker_path = current_dir
        .join("target")
        .join("debug")
        .join("examples")
        .join("math_worker.exe");
    
    let mut worker = ParentWorker::spawn_command(
        worker_path.to_str().expect("Invalid path")
    )?;
    
    // start() waits for child to be ready before returning
    worker.start().await?;
    println!("Worker started!\n");
    
    println!("Calling remote functions...\n");
    
    // Call add
    let result: i64 = worker.call("add", vec![
        serde_json::json!(10),
        serde_json::json!(20),
    ]).await?;
    println!("add(10, 20) = {}", result);
    
    // Call multiply
    let result: i64 = worker.call("multiply", vec![
        serde_json::json!(7),
        serde_json::json!(8),
    ]).await?;
    println!("multiply(7, 8) = {}", result);
    
    // Call factorial
    let result: u64 = worker.call("factorial", vec![
        serde_json::json!(5),
    ]).await?;
    println!("factorial(5) = {}", result);
    
    // Call fibonacci
    let result: u64 = worker.call("fibonacci", vec![
        serde_json::json!(10),
    ]).await?;
    println!("fibonacci(10) = {}", result);
    
    println!("\nDone! Stopping worker...");
    worker.stop().await;
    
    Ok(())
}
