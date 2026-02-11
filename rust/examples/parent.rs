//! Parent example - calls the math worker
//! 
//! First run: cargo run --example math_worker
//! Then run: cargo run --example parent

use multifrost::{ParentWorker, MultifrostError};

#[tokio::main]
async fn main() -> Result<(), MultifrostError> {
    // Connect to running service (run math_worker first)
    println!("Connecting to math-service...");
    let mut worker = ParentWorker::connect("math-service", 5000).await?;
    worker.start().await?;
    
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
    
    println!("\nDone!");
    worker.stop().await;
    
    Ok(())
}
