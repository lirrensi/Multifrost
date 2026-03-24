//! Connect example - connects to an existing v5 service peer.

use multifrost::{call, connect};
use std::env;

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let target = flag_value(&args, "--target").unwrap_or_else(|| "math-service".to_string());
    let timeout_ms = flag_value(&args, "--timeout-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10_000);

    println!("Connecting to {target}...");
    let connection = connect(&target, timeout_ms).await?;
    println!(
        "Connected as {} to default target {}\n",
        connection.peer_id(),
        target
    );
    let handle = connection.handle();
    handle.start().await?;
    println!(
        "Router registry sees {target}: {}\n",
        handle.query_peer_exists(&target).await?
    );

    println!("Calling remote functions...\n");

    let result: i64 = call!(handle, add(10, 20)).await?;
    println!("add(10, 20) = {}", result);

    let result: i64 = call!(handle, multiply(7, 8)).await?;
    println!("multiply(7, 8) = {}", result);

    use serde_json::json;
    let result: i64 = handle.call("factorial", vec![json!(5)]).await?;
    println!("factorial(5) = {}", result);

    println!("\nDone! Stopping connection...");
    handle.stop().await;

    Ok(())
}
