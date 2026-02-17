# Multifrost Rust Quick Examples

Get started with the Rust implementation of Multifrost IPC library.

## Installation

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Create new project
cargo new multifrost_ipc
cd multifrost_ipc

# Add dependencies to Cargo.toml
cat > Cargo.toml << EOF
[package]
name = "multifrost_ipc"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
rmp-serde = "1.1"
zmq = "0.10"
uuid = { version = "1.6", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1.0"
anyhow = "1.0"
EOF

# Build the project
cargo build
```

## Quick Start: Parent-Child Example

Create a child worker with a callable method:

```rust
// src/math_worker.rs
use multifrost_ipc::{ChildWorker, WorkerMethods};

pub struct MathWorker;

impl WorkerMethods for MathWorker {
    fn list_functions(&self) -> Vec<String> {
        vec!["add".to_string(), "multiply".to_string()]
    }

    fn call_method(&self, name: &str, args: &[serde_json::Value]) -> Result<serde_json::Value, anyhow::Error> {
        match name {
            "add" => {
                let a: i64 = args[0].as_i64().ok_or_else(|| anyhow::anyhow!("Invalid argument"))?;
                let b: i64 = args[1].as_i64().ok_or_else(|| anyhow::anyhow!("Invalid argument"))?;
                Ok((a + b).into())
            }
            "multiply" => {
                let a: i64 = args[0].as_i64().ok_or_else(|| anyhow::anyhow!("Invalid argument"))?;
                let b: i64 = args[1].as_i64().ok_or_else(|| anyhow::anyhow!("Invalid argument"))?;
                Ok((a * b).into())
            }
            _ => Err(anyhow::anyhow!("Unknown function: {}", name)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let worker = ChildWorker::new("math-worker");
    worker.run().await?;
    Ok(())
}
```

Create a parent that calls the child:

```rust
// src/parent.rs
use multifrost_ipc::{ParentWorker, SpawnOptions};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Spawn the child worker
    let mut worker = ParentWorker::spawn("./target/debug/math_worker", None, SpawnOptions::default()).await?;

    // Start the worker (lifecycle on worker due to Rust ownership model)
    worker.start().await?;

    // Get a handle for calls
    let handle = worker.handle();

    // Call methods via the handle
    let result1 = handle.call("add", vec![1.into(), 2.into()], None).await?;
    println!("1 + 2 = {}", result1.as_i64().unwrap());

    let result2 = handle.call("multiply", vec![4.into(), 7.into()], None).await?;
    println!("4 * 7 = {}", result2.as_i64().unwrap());

    // Clean up (lifecycle on worker)
    worker.stop().await;
    Ok(())
}
```

Run the example:

```bash
# Build the child worker
cargo build --bin math_worker

# Start the child worker in one terminal
cargo run --bin math_worker

# In another terminal, run the parent
cargo run --bin parent
```

## Worker -> Handle Pattern (v4)

Starting with v4, the API separates process definition (Worker) from runtime interface (Handle):

```
Worker = config/state (holds socket, process, registry internally)
Handle = lightweight API view (call interface)
```

### Rust-Specific Pattern

In Rust, the Handle provides the call interface, but lifecycle methods (`start()`/`stop()`) remain on the worker due to Rust's ownership model requiring mutable access:

```rust
// Lifecycle on worker (requires mutable access)
let mut worker = ParentWorker::spawn(...).await?;
worker.start().await?;

// Call interface via handle (borrows worker)
let handle = worker.handle();
let result = handle.call("add", &[1.into(), 2.into()], None).await?;

// Cleanup on worker
worker.stop().await;
```

### Handle Methods

```rust
// Call a remote method
let result = handle.call("add", vec![1.into(), 2.into()], None).await?;

// Call with timeout
let result = handle.call_with_timeout("add", vec![1.into(), 2.into()], Duration::from_secs(5)).await?;

// Check health
let healthy = handle.is_healthy();
let circuit_open = handle.circuit_open();

// Get metrics
let metrics = handle.metrics();
```

## Rust-Specific Features

### Async/Await with Tokio

The Rust implementation uses `tokio` as the async runtime:

```rust
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut worker = ParentWorker::spawn("./worker", None, SpawnOptions::default()).await?;
    worker.start().await?;

    // All I/O is async - use await for non-blocking calls
    let handle = worker.handle();
    let result = handle.call("add", vec![1.into(), 2.into()], None).await?;

    worker.stop().await;
    Ok(())
}
```

### Shared State with Arc<RwLock<>>

Use `Arc<RwLock<T>>` for shared state across async tasks:

```rust
struct ParentState {
    pending_requests: std::collections::HashMap<String, PendingRequest>,
    consecutive_failures: usize,
    circuit_state: CircuitState,
}

struct ParentWorker {
    state: Arc<std::sync::RwLock<ParentState>>,
    socket: Arc<zmq::DealerSocket>,
}

// Read access (concurrent)
let state = self.state.read().await?;
let is_healthy = state.consecutive_failures < state.threshold;

// Write access (exclusive)
let mut state = self.state.write().await?;
state.consecutive_failures += 1;
```

### Trait-Based Method Registration

Implement the `WorkerMethods` trait for your worker:

```rust
pub trait WorkerMethods: Send + Sync {
    fn list_functions(&self) -> Vec<String>;
    fn call_method(&self, name: &str, args: &[serde_json::Value]) -> Result<serde_json::Value, anyhow::Error>;
}

// Your worker implements the trait
pub struct MathWorker;

impl WorkerMethods for MathWorker {
    fn list_functions(&self) -> Vec<String> {
        vec!["add".to_string(), "multiply".to_string()]
    }

    fn call_method(&self, name: &str, args: &[serde_json::Value]) -> Result<serde_json::Value, anyhow::Error> {
        match name {
            "add" => Ok((args[0].as_i64().unwrap() + args[1].as_i64().unwrap()).into()),
            "multiply" => Ok((args[0].as_i64().unwrap() * args[1].as_i64().unwrap()).into()),
            _ => Err(anyhow::anyhow!("Unknown function: {}", name)),
        }
    }
}
```

### Error Handling with thiserror

Use `Result<T, E>` and `thiserror` for structured errors:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ComlinkError {
    #[error("ZeroMQ error: {0}")]
    Zmq(#[from] zmq::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] rmp_serde::encode::Error),

    #[error("Deserialization error: {0}")]
    Deserialization(#[from] rmp_serde::decode::Error),

    #[error("Circuit breaker is open after {0} consecutive failures")]
    CircuitOpenError(usize),

    #[error("Timeout waiting for response")]
    TimeoutError,
}

// Use the ? operator to propagate errors
async fn call_method(&self, name: &str) -> Result<serde_json::Value, ComlinkError> {
    let message = Message::create_call(name, args)?;
    let bytes = rmp_serde::to_vec(&message)?;  // Propagates serialization errors
    self.socket.send_multipart(...).await?;  // Propagates ZMQ errors
    Ok(result)
}
```

### Message Types with Enums

Use Rust enums for type-safe message handling:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Call {
        id: String,
        function: String,
        args: Vec<serde_json::Value>,
        namespace: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        client_name: Option<String>,
    },
    Response {
        id: String,
        result: serde_json::Value,
    },
    Error {
        id: String,
        error: String,
    },
    Stdout {
        output: String,
    },
    Stderr {
        output: String,
    },
    Heartbeat {
        id: String,
    },
}

// Exhaustive pattern matching
match message {
    Message::Call { id, function, args, .. } => {
        // Handle call
    }
    Message::Response { id, result } => {
        // Handle response
    }
    Message::Error { id, error } => {
        // Handle error
    }
    _ => {}
}
```

## Connect Mode

Register a service and connect from a parent:

```rust
// worker.rs
use multifrost_ipc::{ChildWorker, WorkerMethods};

pub struct MathWorker;

impl WorkerMethods for MathWorker {
    fn list_functions(&self) -> Vec<String> {
        vec!["add".to_string(), "multiply".to_string()]
    }

    fn call_method(&self, name: &str, args: &[serde_json::Value]) -> Result<serde_json::Value, anyhow::Error> {
        match name {
            "add" => Ok((args[0].as_i64().unwrap() + args[1].as_i64().unwrap()).into()),
            _ => Err(anyhow::anyhow!("Unknown function: {}", name)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let worker = ChildWorker::new_with_service_id("math-service", "comlink_ipc_v4");
    worker.run().await?;
    Ok(())
}
```

```rust
// parent.rs
use multifrost_ipc::ParentWorker;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Connect to the existing service
    let mut worker = ParentWorker::connect("math-service", SpawnOptions::default()).await?;

    worker.start().await?;

    let handle = worker.handle();
    let result = handle.call("add", vec![5.into(), 3.into()], None).await?;
    println!("5 + 3 = {}", result.as_i64().unwrap());

    worker.stop().await;
    Ok(())
}
```

## Common Patterns

### Error Handling

```rust
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut worker = ParentWorker::spawn("./worker", None, SpawnOptions::default()).await?;
    worker.start().await?;

    let handle = worker.handle();
    match handle.call("add", vec![1.into(), 2.into()], None).await {
        Ok(result) => println!("Result: {}", result),
        Err(ComlinkError::TimeoutError) => println!("Request timed out"),
        Err(ComlinkError::CircuitOpenError(count)) => {
            println!("Circuit breaker is open after {} failures", count)
        }
        Err(e) => println!("Error: {}", e),
    }

    worker.stop().await;
    Ok(())
}
```

### Method with Options

```rust
// Call with custom timeout
let result = handle.call_with_timeout(
    "add",
    vec![1.into(), 2.into()],
    Duration::from_secs(5)
).await?;
```

### List Available Methods

```rust
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut worker = ParentWorker::spawn("./worker", None, SpawnOptions::default()).await?;
    worker.start().await?;

    // List methods on the child
    let methods = worker.list_functions().await?;
    println!("Available methods: {:?}", methods);

    worker.stop().await;
    Ok(())
}
```

### Metrics Collection

```rust
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut worker = ParentWorker::spawn("./worker", None, SpawnOptions::default()).await?;
    worker.start().await?;

    // Get metrics from handle
    let handle = worker.handle();
    let metrics = handle.metrics();
    println!("Total requests: {}", metrics.total_requests);
    println!("Success rate: {}", metrics.success_rate());
    println!("Average latency: {}ms", metrics.average_latency());

    worker.stop().await;
    Ok(())
}
```

### Health Checks

```rust
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut worker = ParentWorker::spawn("./worker", None, SpawnOptions::default()).await?;
    worker.start().await?;

    // Check if worker is healthy via handle
    let handle = worker.handle();
    println!("Healthy: {}", handle.is_healthy());
    println!("Circuit open: {}", handle.circuit_open());

    worker.stop().await;
    Ok(())
}
```

## Key Concepts

### ParentWorker
- **Purpose**: Initiates calls and manages child lifecycle
- **Modes**: `spawn()` (creates new process) or `connect()` (connects to existing service)
- **Lifecycle**: `start()`, `stop()` (on worker due to ownership model)

### Handle
- **Purpose**: Lightweight API view for call interface
- **Methods**: `call()`, `call_with_timeout()`, `is_healthy()`, `circuit_open()`, `metrics()`
- **Note**: Lifecycle methods remain on worker in Rust

### ChildWorker
- **Purpose**: Exposes callable methods and handles requests
- **Methods**: Implement `WorkerMethods` trait
- **Modes**: Can register with `service_id` for connect mode

### Spawn Mode
- Parent finds free port and binds DEALER socket
- Parent spawns child with `COMLINK_ZMQ_PORT` environment variable
- Parent owns child process lifecycle
- Heartbeat monitoring (default: every 5s, timeout 3s)

### Connect Mode
- Child registers with service registry (`~/.multifrost/services.json`)
- Parent discovers service and connects
- Better for long-running services

### Circuit Breaker
- Tracks consecutive failures (default: 5)
- Opens circuit after threshold
- Resets on successful call
- Prevents cascading failures

### Heartbeat Monitoring
- Parent sends periodic heartbeats to child (spawn mode only)
- Calculates round-trip time (RTT)
- Trips circuit breaker on missed heartbeats (default: 3 consecutive misses)

### Memory Safety
- **Ownership system**: No garbage collection
- **Type system**: Compile-time guarantees
- **Arc<RwLock<>>**: Safe shared state across tasks
- **No unsafe code**: All memory safety guaranteed

## Cross-Language Usage

Rust parent calling Python child:

```rust
// parent.rs
use multifrost_ipc::ParentWorker;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Spawn Python worker
    let mut worker = ParentWorker::spawn("./math_worker.py", Some("python"), SpawnOptions::default()).await?;

    worker.start().await?;

    // Call Python method
    let handle = worker.handle();
    let result = handle.call("factorial", vec![10.into()], None).await?;
    println!("Factorial: {}", result.as_u64().unwrap());

    worker.stop().await;
    Ok(())
}
```

```python
# math_worker.py
from multifrost import ChildWorker

class MathWorker(ChildWorker):
    def factorial(self, n: int) -> int:
        if n <= 1:
            return 1
        return n * self.factorial(n - 1)

if __name__ == "__main__":
    worker = MathWorker()
    worker.run()
```

Rust parent calling JavaScript child:

```rust
// parent.rs
use multifrost_ipc::ParentWorker;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Spawn JavaScript worker
    let mut worker = ParentWorker::spawn("./math_worker.js", Some("node"), SpawnOptions::default()).await?;

    worker.start().await?;

    // Call JavaScript method
    let handle = worker.handle();
    let result = handle.call("add", vec![21.into(), 3.into()], None).await?;
    println!("21 + 3 = {}", result.as_i64().unwrap());

    worker.stop().await;
    Ok(())
}
```

```javascript
// math_worker.js
const { ChildWorker } = require("./src/multifrost");

class MathWorker extends ChildWorker {
    add(a, b) {
        return a + b;
    }
}

const worker = new MathWorker();
worker.run();
```

## Troubleshooting

**Child exits immediately:**
- Check `COMLINK_ZMQ_PORT` environment variable
- Verify port is in range 1024-65535
- Check stderr for error messages
- Ensure executable path is correct

**Async functions don't work:**
- Ensure you're using `#[tokio::main]` macro
- Use `await` for non-blocking operations
- Check that methods are `pub` (must be public to be callable)

**Circuit breaker trips:**
- Check heartbeat configuration
- Verify child process is responding
- Review metrics for error patterns
- Consider increasing failure threshold

**Serialization errors:**
- Ensure message types derive `Serialize` and `Deserialize`
- Verify `serde_json::Value` is used for arguments
- Check that all fields are properly serialized

**Connection refused:**
- Verify ZeroMQ socket is binding/connecting to correct port
- Check that child process is running
- Ensure port is not already in use

**Memory leaks:**
- Check that `Arc` references are properly managed
- Verify `RwLock` is not held too long
- Ensure pending requests are cleaned up

For more details, see the [full architecture documentation](./arch.md).
