# Multifrost Rust - Frequently Asked Questions

> This FAQ covers the Rust implementation of Multifrost, a type-safe, async-native IPC library for parent-child process communication.

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Arc<RwLock<>> Concurrency Patterns](#arcrwlock-concurrency-patterns)
3. [Trait-Based Method Registration](#trait-based-method-registration)
4. [Spawn vs Connect Mode](#spawn-vs-connect-mode)
5. [Common Gotchas and Pitfalls](#common-gotchas-and-pitfalls)
6. [Performance Considerations](#performance-considerations)
7. [Debugging Tips](#debugging-tips)
8. [Troubleshooting](#troubleshooting)
9. [Best Practices](#best-practices)
10. [Cross-Language Communication](#cross-language-communication)

---

## Getting Started

### Q: What are the Rust version and dependencies?

**A:** The Rust implementation requires:

- **Rust edition**: 2021
- **Minimum Rust version**: 1.70.0 (compatible with tokio 1.x)
- **Key dependencies**:
  - `tokio` (1.x) - Async runtime
  - `zeromq` (0.4) - ZeroMQ bindings
  - `rmp-serde` (1.3) - MessagePack serialization
  - `serde` (1.x) - Serialization framework
  - `thiserror` (1.x) - Error handling
  - `async-trait` (0.1) - Async trait support

Install dependencies:

```bash
cd rust
cargo build
```

### Q: How do I create a simple worker?

**A:** Create a struct that implements the `ChildWorker` trait:

```rust
use multifrost::{ChildWorker, ChildWorkerContext, Result, run_worker};
use async_trait::async_trait;
use serde_json::Value;

struct MyWorker;

#[async_trait]
impl ChildWorker for MyWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        match function {
            "greet" => {
                let name = args.get(0).and_then(|v| v.as_str()).unwrap_or("world");
                Ok(serde_json::json!(format!("Hello, {}!", name)))
            }
            _ => Err(MultifrostError::FunctionNotFound(function.to_string())),
        }
    }
}

#[tokio::main]
async fn main() {
    let ctx = ChildWorkerContext::new().with_service_id("my-service");
    run_worker(MyWorker, ctx).await;
}
```

### Q: How do I spawn a worker from a parent process?

**A:** Use `ParentWorkerBuilder` for a clean, ergonomic API:

```rust
use multifrost::{ParentWorker, ParentWorkerBuilder};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Spawn mode: parent creates child process
    let mut worker = ParentWorkerBuilder::spawn(
        "",  // service_id (unused in spawn mode)
        "./math_worker"  // path to worker executable
    )
    .auto_restart(false)
    .default_timeout(Duration::from_secs(30))
    .stdout_handler(|output| println!("[STDOUT]: {}", output))
    .build()
    .await?;

    // Wait for worker to be ready
    worker.start().await?;
    println!("Worker started!");

    // Call remote methods
    let result: i64 = worker.call!(add(10, 20)).await?;
    println!("add(10, 20) = {}", result);

    worker.stop().await;
    Ok(())
}
```

### Q: How do I use the ergonomic `call!` macro?

**A:** The `call!` macro provides compile-time checks and type safety:

```rust
// Define a function with a return type annotation
let result: i64 = worker.call!(add(10, 20)).await?;
let result: u64 = worker.call!(factorial(5)).await?;

// The macro generates the equivalent code:
// let result: i64 = worker.call("add", vec![json!(10), json!(20)], None).await?;
```

### Q: How do I run examples?

**A:** Run examples directly with cargo:

```bash
# Run spawn mode example (parent spawns child)
cargo run --example spawn

# Run child worker example
cargo run --example math_worker

# Run connect mode example
cargo run --example connect
```

---

## Arc<RwLock<>> Concurrency Patterns

### Q: Why does Multifrost use `Arc<RwLock<T>>` for shared state?

**A:** The Rust implementation uses `Arc<RwLock<T>>` for thread-safe shared state across async tasks. This pattern provides:

- **Arc (Atomic Reference Counting)**: Enables multiple ownership across tokio tasks without `Send + Sync` restrictions
- **RwLock (Read-Write Lock)**: Allows concurrent reads while providing exclusive write access
- **Tokio Compatibility**: `tokio::sync::RwLock` is async-safe and integrates with tokio's runtime

```rust
struct ParentWorker {
    state: Arc<RwLock<ParentState>>,  // Shared mutable state
    socket: Arc<DealerSocket>,         // Shared socket across tasks
    metrics: Arc<Metrics>,             // Shared metrics collection
    logger: Arc<StructuredLogger>,     // Shared logging
}

// Read access (multiple tasks can read concurrently)
let state = self.state.read().await?;
let is_healthy = state.consecutive_failures < state.threshold;

// Write access (exclusive access, one task at a time)
let mut state = self.state.write().await?;
state.consecutive_failures += 1;
```

### Q: When should I use read vs write locks?

**A:** Use read locks (`read().await`) when you only need to read data without modification. Use write locks (`write().await`) when you need to modify shared state.

**Read-heavy workloads (recommended):**
```rust
// Multiple tasks can read concurrently
let metrics = self.metrics.read().await;
let avg_latency = metrics.calculate_avg_latency();
```

**Write-heavy workloads:**
```rust
// Only one task can write at a time
let mut state = self.state.write().await;
state.consecutive_failures = 0;
state.circuit_state = CircuitState::Closed;
```

### Q: How do I avoid deadlocks with RwLock?

**A:** Follow these patterns to prevent deadlocks:

1. **Hold locks for the shortest time possible**
```rust
// GOOD: Acquire and release quickly
let result = {
    let state = self.state.read().await;
    state.consecutive_failures
};
```

2. **Never acquire locks in a different order**
```rust
// BAD: Locks acquired in different orders
let mut state1 = self.state1.write().await;
let mut state2 = self.state2.write().await;  // DEADLOCK!

// GOOD: Always acquire locks in the same order
let mut state1 = self.state1.write().await;
let mut state2 = self.state2.write().await;
```

3. **Use try_write() for non-blocking access**
```rust
// Try to acquire write lock, fail gracefully if locked
if let Ok(mut state) = self.state.try_write() {
    state.consecutive_failures += 1;
} else {
    // Handle contention
}
```

### Q: How do I share state between spawned tasks?

**A:** Use `Arc<RwLock<T>>` to share mutable state across spawned tasks:

```rust
struct ParentState {
    pending_requests: HashMap<String, PendingRequest>,
    consecutive_failures: usize,
}

struct ParentWorker {
    state: Arc<RwLock<ParentState>>,
}

// Spawn multiple tasks that share state
let state = Arc::new(RwLock::new(ParentState::default()));
let state_clone = Arc::clone(&state);

// Task 1: Heartbeat monitoring (read-heavy)
let handle1 = tokio::spawn(async move {
    let state = state_clone.read().await;
    let is_healthy = state.consecutive_failures < 5;
});

// Task 2: Request handler (write-heavy)
let handle2 = tokio::spawn(async move {
    let mut state = state_clone.write().await;
    state.consecutive_failures += 1;
});
```

---

## Trait-Based Method Registration

### Q: How does trait-based method registration work?

**A:** The Rust implementation uses traits instead of reflection for type-safe method registration:

```rust
#[async_trait]
impl ChildWorker for MathWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        match function {
            "add" => {
                let a = args[0].as_i64().unwrap();
                let b = args[1].as_i64().unwrap();
                Ok((a + b).into())
            }
            _ => Err(MultifrostError::FunctionNotFound(function.to_string())),
        }
    }
}
```

**Benefits:**
- **Compile-time checking**: Method names are checked at compile time
- **Type safety**: Argument types are enforced by the compiler
- **No runtime overhead**: No reflection or dynamic dispatch needed
- **Easy testing**: Can test methods without IPC layer

### Q: How do I define custom worker traits?

**A:** Define a trait for your worker methods:

```rust
#[async_trait]
pub trait MathWorkerMethods: Send + Sync {
    fn list_functions(&self) -> Vec<String>;
    async fn add(&self, a: i64, b: i64) -> Result<i64, ComlinkError>;
    async fn multiply(&self, a: i64, b: i64) -> Result<i64, ComlinkError>;
    async fn factorial(&self, n: u64) -> Result<u64, ComlinkError>;
}

struct MathWorker;

impl MathWorkerMethods for MathWorker {
    fn list_functions(&self) -> Vec<String> {
        vec!["add", "multiply", "factorial"].to_vec()
    }

    async fn add(&self, a: i64, b: i64) -> Result<i64, ComlinkError> {
        Ok(a + b)
    }

    async fn multiply(&self, a: i64, b: i64) -> Result<i64, ComlinkError> {
        Ok(a * b)
    }

    async fn factorial(&self, n: u64) -> Result<u64, ComlinkError> {
        if n > 20 {
            return Err(ComlinkError::FunctionNotFoundError("factorial".to_string()));
        }
        let result: u64 = (1..=n).product();
        Ok(result)
    }
}
```

### Q: How do I handle errors in worker methods?

**A:** Use the `Result<T, ComlinkError>` type for error handling:

```rust
#[async_trait]
impl ChildWorker for MyWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        match function {
            "divide" => {
                let a = args[0].as_f64()
                    .ok_or_else(|| ComlinkError::InvalidArgument("First arg must be a number".to_string()))?;
                let b = args[1].as_f64()
                    .ok_or_else(|| ComlinkError::InvalidArgument("Second arg must be a number".to_string()))?;

                if b == 0.0 {
                    return Err(ComlinkError::DivideByZero);
                }

                Ok(serde_json::json!(a / b))
            }
            _ => Err(MultifrostError::FunctionNotFound(function.to_string())),
        }
    }
}
```

### Q: How do I expose multiple worker instances?

**A:** Implement `ChildWorker` for each worker struct:

```rust
struct MathWorker;
struct TextWorker;
struct FileWorker;

#[async_trait]
impl ChildWorker for MathWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        // Math operations
    }
}

#[async_trait]
impl ChildWorker for TextWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        // Text operations
    }
}

#[async_trait]
impl ChildWorker for FileWorker {
    async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
        // File operations
    }
}
```

---

## Spawn vs Connect Mode

### Q: What is the difference between spawn and connect modes?

**A:**

| Feature | Spawn Mode | Connect Mode |
|---------|-----------|--------------|
| **Process Management** | Parent spawns child | Parent connects to running child |
| **Port Binding** | Parent finds free port | Child binds its own port |
| **Environment Variables** | `COMLINK_ZMQ_PORT` set by parent | `COMLINK_ZMQ_PORT` set by child |
| **Connection Flow** | Parent → Child (sync) | Parent → Registry → Child |
| **Use Case** | Spawning worker processes | Connecting to services |
| **Auto-Restart** | Supported | Supported |

**Spawn Mode:**
```rust
// Parent spawns child process
let mut worker = ParentWorkerBuilder::spawn(
    "",  // service_id
    "./my_worker"  // path to executable
)
.build()
.await?;

worker.start().await?;  // Wait for child to connect
let result: i64 = worker.call!(add(10, 20)).await?;
```

**Connect Mode:**
```rust
// Parent connects to running child
let mut worker = ParentWorkerBuilder::connect(
    "my-service",  // service_id
    None,  // no executable path
)
.build()
.await?;

worker.start().await?;  // Wait for connection
let result: i64 = worker.call!(add(10, 20)).await?;
```

### Q: How does spawn mode work internally?

**A:** Spawn mode involves these steps:

1. **Find free port** on the parent
2. **Bind DEALER socket** on the parent
3. **Spawn child process** with `COMLINK_ZMQ_PORT` environment variable
4. **Wait for child connection** (with timeout)
5. **Start heartbeat monitoring**

```rust
// From rust/docs/arch.md

pub async fn spawn(
    script_path: &str,
    executable: Option<&str>,
    options: SpawnOptions,
) -> Result<Self, ComlinkError> {
    // 1. Find free port
    let port = find_free_port()?;

    // 2. Bind DEALER socket
    let socket = DealerSocket::bind(&format!("tcp://*:{}", port))?;

    // 3. Spawn child process
    let mut cmd = Command::new(executable.unwrap_or("python"));
    cmd.arg(script_path);
    cmd.env("COMLINK_ZMQ_PORT", port.to_string());
    let child = cmd.spawn()?;

    // 4. Wait for child to connect
    tokio::time::timeout(
        Duration::from_secs(5),
        wait_for_connection(&socket)
    ).await??;

    // 5. Start heartbeat loop
    let heartbeat_handle = tokio::spawn(heartbeat_loop(...));

    Ok(Self { socket, state, child, heartbeat_handle, ... })
}
```

### Q: How does connect mode work internally?

**A:** Connect mode uses a service registry for discovery:

1. **Register child** in registry with service_id and PID
2. **Parent queries registry** for service location
3. **Parent connects** to child's socket

```rust
// From rust/docs/arch.md

pub async fn connect(service_id: &str, executable: Option<&str>) -> Result<Self, ComlinkError> {
    // 1. Query registry for service location
    let service = registry::find_service(service_id)?;

    // 2. Connect to child's DEALER socket
    let socket = DealerSocket::connect(&service.address)?;

    // 3. Verify connection
    let mut state = ParentState::new();
    state.child_pid = Some(service.pid);

    Ok(Self { socket, state, ... })
}
```

### Q: Can I use both spawn and connect in the same application?

**A:** Yes! You can mix spawn and connect modes:

```rust
// Spawn a local worker
let local_worker = ParentWorkerBuilder::spawn(
    "",  // service_id
    "./local_worker"
).build()
.await?;

// Connect to a remote service
let remote_worker = ParentWorkerBuilder::connect(
    "remote-service",
    None,
).build()
.await?;

// Call both workers
let local_result: i64 = local_worker.call!(add(10, 20)).await?;
let remote_result: i64 = remote_worker.call!(compute(30, 40)).await?;
```

### Q: How do I configure spawn mode options?

**A:** Use `SpawnOptions` for fine-grained control:

```rust
let mut worker = ParentWorkerBuilder::spawn(
    "",  // service_id
    "./worker"
)
.auto_restart(true)  // Restart on crash
.default_timeout(Duration::from_secs(30))  // Default request timeout
.heartbeat_interval(5.0)  // Send heartbeat every 5 seconds
.heartbeat_timeout(3.0)  // Wait 3 seconds for heartbeat response
.heartbeat_max_misses(3)  // Trip circuit after 3 missed heartbeats
.stdout_handler(|output| println!("[STDOUT]: {}", output))  // Custom stdout handler
.stderr_handler(|output| eprintln!("[STDERR]: {}", output))
.build()
.await?;
```

---

## Common Gotchas and Pitfalls

### Q: Why does my worker not receive messages?

**A:** Common causes:

1. **Socket not bound/connecting**: Ensure socket is properly bound in spawn mode or connected in connect mode
2. **Wrong port**: Check that `COMLINK_ZMQ_PORT` is set correctly
3. **Network issues**: Verify TCP connectivity between processes

**Debugging:**
```rust
// Check socket state
let state = worker.state.read().await;
println!("Child PID: {:?}", state.child_pid);
println!("Circuit state: {:?}", state.circuit_state);
```

### Q: Why is my request timing out?

**A:** Common causes:

1. **Function not found**: Check method name spelling
2. **Invalid arguments**: Verify argument types and count
3. **Worker crashed**: Check circuit breaker state

**Debugging:**
```rust
// Enable detailed logging
worker.set_log_level(LogLevel::Debug);

// Check metrics
let metrics = worker.metrics.snapshot();
println!("Error rate: {:.2}%", metrics.error_rate * 100.0);
println!("Avg latency: {:.2}ms", metrics.request_metrics.avg);
```

### Q: Why is my Arc clone causing memory leaks?

**A:** `Arc` uses reference counting, so cloning is O(1) and doesn't cause leaks. It only increments the reference count.

```rust
// O(1) clone - no performance penalty
let state = Arc::new(RwLock::new(ParentState::default()));
let state_clone = Arc::clone(&state);

// When all clones are dropped, memory is freed
```

### Q: Why is my RwLock acquisition hanging?

**A:** Possible causes:

1. **Write lock held by another task**: Check for long-running write operations
2. **Deadlock**: Ensure locks are acquired in a consistent order
3. **Task not responding**: Check if the holding task is stuck

**Debugging:**
```rust
// Use try_write() to avoid hanging
if let Ok(mut state) = self.state.try_write() {
    // Modify state
} else {
    // Handle contention
}

// Use tokio-console for async debugging
// cargo install tokio-console
// tokio-console
```

### Q: Why am I getting "Function not found" errors?

**A:** Verify:

1. **Method name matches exactly** (case-sensitive)
2. **Worker is running** and has methods registered
3. **Service ID is correct** (for connect mode)

```rust
// List available methods
let methods: Vec<String> = worker.call("list_functions", vec![], None).await?;
println!("Available methods: {:?}", methods);
```

### Q: Why is my child process exiting immediately?

**A:** Common causes:

1. **Port binding failed**: Check port availability
2. **Invalid environment variables**: Ensure `COMLINK_ZMQ_PORT` is set
3. **Missing dependencies**: Verify all required dependencies are installed

**Debugging:**
```rust
// Check child process status
let state = worker.state.read().await;
if let Some(child) = &worker.child {
    match child.try_wait() {
        Ok(Some(status)) => println!("Child exited with status: {:?}", status),
        Ok(None) => println!("Child is still running"),
        Err(e) => println!("Failed to check child status: {:?}", e),
    }
}
```

---

## Performance Considerations

### Q: How does Multifrost achieve performance?

**A:** Several optimizations:

1. **Zero-copy message handling**: Reference slices instead of cloning
2. **Async non-blocking I/O**: Uses tokio for efficient event loop
3. **Efficient data structures**: `HashMap` for O(1) lookups, `VecDeque` for sliding windows
4. **Arc for shared ownership**: Minimal overhead with reference counting

```rust
// Zero-copy: reference, not copy
let parts = socket.recv_multipart(0).await?;
let message_bytes = &parts[1];  // Reference

// Efficient data structures
let pending_requests: HashMap<String, PendingRequest> = ...;  // O(1) lookup
let latencies: VecDeque<f64> = ...;  // O(1) push/pop
```

### Q: What is the impact of RwLock on performance?

**A:** RwLock is optimized for read-heavy workloads:

- **Read locks** are fast (no blocking when no writers)
- **Write locks** block all readers and other writers
- **Contended writes** have higher overhead

**Recommendations:**
- Use read locks for metrics, health checks
- Use write locks sparingly for state updates
- Consider atomic types for simple counters

```rust
// Read-heavy: multiple readers can access concurrently
let metrics = self.metrics.read().await;
let avg_latency = metrics.calculate_avg_latency();

// Write-heavy: only one writer at a time
let mut state = self.state.write().await;
state.consecutive_failures += 1;
```

### Q: How do I optimize message serialization?

**A:** Use efficient serialization strategies:

1. **Reuse buffers** to avoid allocation
2. **Pre-allocate Vec capacity** when possible
3. **Use primitive types** instead of complex structs

```rust
// Pre-allocate capacity
let args: Vec<Value> = Vec::with_capacity(2);
args.push(json!(1));
args.push(json!(2));

// Reuse buffers for repeated serialization
let mut buffer = Vec::new();
let bytes = to_vec(&message, &mut buffer)?;
```

### Q: How does Arc overhead compare to other approaches?

**A:** Arc overhead is minimal:

- **Reference count**: 1-4 bytes per Arc (atomic operations)
- **Pointer**: 8 bytes on 64-bit systems
- **Total overhead**: ~16 bytes per shared reference

**Comparison:**
- `Arc<T>`: 16 bytes (reference counting)
- `Box<T>`: 8 bytes (ownership transfer)
- `Rc<T>`: 16 bytes (reference counting, not Send)
- `Mutex<T>`: 16 bytes + lock overhead

### Q: How do I measure performance?

**A:** Use the built-in metrics:

```rust
// Get performance metrics
let metrics = worker.metrics.snapshot();

println!("Request Metrics:");
println!("  Avg Latency: {:.2}ms", metrics.request_metrics.avg);
println!("  Min Latency: {:.2}ms", metrics.request_metrics.min);
println!("  Max Latency: {:.2}ms", metrics.request_metrics.max);
println!("  P50 Latency: {:.2}ms", metrics.request_metrics.p50);
println!("  P95 Latency: {:.2}ms", metrics.request_metrics.p95);
println!("  P99 Latency: {:.2}ms", metrics.request_metrics.p99);

println!("Error Rate: {:.2}%", metrics.error_rate * 100.0);

println!("Circuit Breaker Events: {}", metrics.circuit_breaker_events.len());

println!("Heartbeat RTT:");
println!("  Min RTT: {:.2}ms", metrics.heartbeat_rtt_ms.min);
println!("  Max RTT: {:.2}ms", metrics.heartbeat_rtt_ms.max);
println!("  Avg RTT: {:.2}ms", metrics.heartbeat_rtt_ms.avg);
```

### Q: How do I optimize for high throughput?

**A:** Several strategies:

1. **Increase heartbeat interval** to reduce monitoring overhead
2. **Use connection pooling** for multiple workers
3. **Batch requests** when possible
4. **Reduce logging verbosity** in production

```rust
// Optimize heartbeat for high throughput
let mut worker = ParentWorkerBuilder::spawn("", "./worker")
    .heartbeat_interval(10.0)  // Send heartbeats every 10s (reduced from 5s)
    .heartbeat_timeout(5.0)    // Wait 5s for response
    .heartbeat_max_misses(5)   // Trip after 5 misses (reduced from 3)
    .build()
    .await?;
```

---

## Debugging Tips

### Q: How do I enable structured logging?

**A:** Use the built-in logger with custom handlers:

```rust
use multifrost::StructuredLogger;

// Create logger with multiple handlers
let logger = StructuredLogger::new()
    .with_handler(default_json_handler())  // JSON format
    .with_handler(default_pretty_handler())  // Pretty print

// Use the logger
logger.log(LogEntry {
    timestamp: Utc::now(),
    level: LogLevel::Info,
    event: LogEvent::WorkerStart,
    metadata: {
        let mut m = HashMap::new();
        m.insert("service_id".to_string(), "my-service".into());
        m
    },
});
```

### Q: How do I debug async code?

**A:** Use tokio's debugging tools:

1. **tokio-console**: Real-time async runtime visualization
2. **tracing**: Structured logging framework
3. **tokio-tracing-opentelemetry**: Distributed tracing

```bash
# Install tokio-console
cargo install tokio-console

# Run with console
tokio-console
```

**Enable console in your code:**
```rust
use tracing_subscriber;

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
}
```

### Q: How do I inspect message traffic?

**A:** Use debug handlers or custom logging:

```rust
// Custom message inspector
worker.set_message_handler(|message| {
    println!("Received message: {:?}", message);
});

// Or use tokio-console to visualize async operations
tokio-console
```

### Q: How do I test IPC communication locally?

**A:** Use the provided examples:

```bash
# Spawn mode test
cargo run --example spawn

# Connect mode test
cargo run --example connect
```

**Write integration tests:**
```rust
#[tokio::test]
async fn test_parent_child_communication() {
    let child = ChildWorker::new("test-service");
    tokio::spawn(async move {
        child.run().await;
    });

    let parent = ParentWorker::connect("test-service").await.unwrap();
    let result = parent.call("add", vec![json!(1), json!(2)], None)
        .await
        .unwrap();
    assert_eq!(result, 3);
}
```

### Q: How do I use breakpoints in async code?

**A:** Debugging async code requires special tools:

1. **lldb** (LLVM debugger):
   ```bash
   lldb cargo run --example spawn
   ```

2. **VS Code with rust-analyzer**: Use the built-in debugger
   ```json
   {
       "type": "lldb",
       "request": "launch",
       "program": "${workspaceFolder}/target/debug/examples/spawn",
       "args": [],
       "cwd": "${workspaceFolder}"
   }
   ```

3. **tokio-console**: For async-specific debugging

### Q: How do I profile performance?

**A:** Use Rust profiling tools:

```bash
# Enable profiling
RUSTFLAGS="-Cprofile-generate=target/release/profile-out" cargo build --release

# Run your application
./target/release/examples/spawn

# Generate flamegraph
cargo flamegraph

# Use cargo-flamegraph for easy profiling
cargo install flamegraph
cargo flamegraph --example spawn
```

---

## Troubleshooting

### Q: How do I fix "port already in use" errors?

**A:** Options:

1. **Use a different port**: Multifrost automatically finds free ports
2. **Kill existing processes**: Find and kill processes using the port
3. **Increase port range**: Configure custom port range

```bash
# Find process using port
netstat -ano | findstr :5555
taskkill /PID <PID> /F

# Or let Multifrost find a free port automatically
```

### Q: How do I fix "Connection refused" errors?

**A:** Check:

1. **Child process is running**: Verify child executable exists
2. **Port is correct**: Check `COMLINK_ZMQ_PORT` matches
3. **Network connectivity**: Use `telnet` or `nc` to test TCP connection

```bash
# Test TCP connectivity
telnet localhost 5555

# Or use nc
nc -zv localhost 5555
```

### Q: How do I fix "Serialization error"?

**A:** Common causes:

1. **Type mismatch**: Ensure argument types match expected types
2. **Missing serde derive**: Add `#[derive(Serialize, Deserialize)]` to structs
3. **Invalid value**: Check argument values are valid

```rust
// Ensure types are compatible
#[derive(Serialize, Deserialize)]
struct MyStruct {
    value: i64,
}

let message = MyStruct { value: 42 };
let bytes = to_vec(&message)?;  // This works
```

### Q: How do I fix "Timeout waiting for response"?

**A:** Options:

1. **Increase timeout**: Use `.default_timeout(Duration::from_secs(60))`
2. **Check worker health**: Verify worker is running and responsive
3. **Review circuit breaker**: Check if circuit is open

```rust
// Increase timeout
let mut worker = ParentWorkerBuilder::spawn("", "./worker")
    .default_timeout(Duration::from_secs(60))  // 60 seconds
    .build()
    .await?;
```

### Q: How do I fix "Module not found" errors?

**A:** Ensure dependencies are properly configured:

```bash
# Check Cargo.toml
cat rust/Cargo.toml

# Run cargo check
cd rust
cargo check

# Clean and rebuild
cargo clean
cargo build
```

### Q: How do I fix "Child process exited with code 1"?

**A:** Check the child process logs for errors:

```rust
// Set up stdout/stderr handlers
worker.stdout_handler(|output| {
    eprintln!("[STDOUT]: {}", output);
});

worker.stderr_handler(|output| {
    eprintln!("[STDERR]: {}", output);
});

// Check child process status
let state = worker.state.read().await;
if let Some(status) = worker.child.as_ref().map(|c| c.try_wait()) {
    match status {
        Ok(Some(exit_status)) => {
            eprintln!("Child exited with: {:?}", exit_status);
        }
        _ => {}
    }
}
```

---

## Best Practices

### Q: How do I structure my worker project?

**A:** Follow this project structure:

```
my_worker/
├── src/
│   ├── main.rs
│   ├── worker.rs
│   └── lib.rs
├── tests/
│   └── integration_tests.rs
├── examples/
│   └── basic_worker.rs
└── Cargo.toml
```

**Example Cargo.toml:**
```toml
[package]
name = "my_worker"
version = "0.1.0"
edition = "2021"

[dependencies]
multifrost = { path = "../../rust" }
async-trait = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

### Q: How do I write unit tests?

**A:** Test worker methods in isolation:

```rust
use multifrost::Result;

#[cfg(test)]
mod tests {
    use super::*;

    struct TestWorker;

    #[tokio::test]
    async fn test_add_method() -> Result<()> {
        let worker = TestWorker;
        let result = worker.add(10, 20).await?;
        assert_eq!(result, 30);
        Ok(())
    }

    #[tokio::test]
    async fn test_divide_by_zero() -> Result<()> {
        let worker = TestWorker;
        let result = worker.divide(10, 0).await;
        assert!(result.is_err());
        Ok(())
    }
}
```

### Q: How do I handle graceful shutdown?

**A:** Implement proper cleanup:

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut worker = ParentWorkerBuilder::spawn("", "./worker").build().await?;

    // Setup signal handlers for graceful shutdown
    let mut shutdown = tokio::signal::ctrl_c();
    tokio::select! {
        _ = shutdown => {
            println!("Shutting down...");
            worker.stop().await;
            println!("Shutdown complete");
        }
        result = worker.run() => {
            println!("Worker error: {:?}", result);
        }
    }

    Ok(())
}
```

### Q: How do I implement proper error handling?

**A:** Use `thiserror` for rich error types:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MyWorkerError {
    #[error("Input validation failed: {0}")]
    InvalidInput(String),

    #[error("Computation failed: {0}")]
    ComputationError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Multifrost error: {0}")]
    Multifrost(#[from] multifrost::ComlinkError),
}

// Use in methods
async fn divide(&self, a: i64, b: i64) -> Result<i64, MyWorkerError> {
    if b == 0 {
        return Err(MyWorkerError::InvalidInput("Division by zero".to_string()));
    }
    Ok(a / b)
}
```

### Q: How do I implement configuration management?

**A:** Use environment variables and config files:

```rust
use std::env;

fn load_config() -> Config {
    Config {
        port: env::var("COMLINK_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5555),
        heartbeat_interval: env::var("HEARTBEAT_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5.0),
        timeout: env::var("TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30),
    }
}

struct Config {
    port: u16,
    heartbeat_interval: f64,
    timeout: u64,
}
```

### Q: How do I implement monitoring and observability?

**A:** Use metrics and structured logging:

```rust
// Metrics collection
let metrics = worker.metrics.snapshot();

// Structured logging
logger.log(LogEntry {
    timestamp: Utc::now(),
    level: LogLevel::Info,
    event: LogEvent::RequestStart,
    metadata: {
        let mut m = HashMap::new();
        m.insert("function".to_string(), "add".into());
        m.insert("args".to_string(), json!(vec![10, 20]));
        m.insert("latency_ms".to_string(), metrics.request_metrics.avg.into());
        m
    },
});

// Export metrics to Prometheus
// metrics.export_to_prometheus("/metrics");
```

### Q: How do I ensure thread safety?

**A:** Follow Rust's ownership and borrowing rules:

1. **Use `Arc<T>` for shared ownership**
2. **Use `RwLock<T>` for shared mutable state**
3. **Avoid `Mutex<T>` unless necessary** (RwLock is usually better for read-heavy workloads)
4. **Use `Send + Sync` bounds** for thread-safe types

```rust
struct SafeState {
    counter: Arc<AtomicUsize>,  // Atomic for simple counters
    shared_data: Arc<RwLock<Vec<String>>>,  // RwLock for complex shared data
}

impl SafeState {
    fn increment(&self) {
        self.counter.fetch_add(1, Ordering::SeqCst);
    }

    fn add_item(&self, item: String) {
        let mut data = self.shared_data.write().unwrap();
        data.push(item);
    }
}
```

---

## Cross-Language Communication

### Q: How does Rust communicate with Python workers?

**A:** Rust uses the same msgpack wire protocol as Python, ensuring compatibility:

1. **Message format**: Both use msgpack (rmp-serde in Rust, msgpack in Python)
2. **Message types**: Call, Response, Error, Heartbeat, Stdout, Stderr
3. **Socket types**: DEALER (parent) / ROUTER (child) in both languages
4. **ZeroMQ bindings**: zeromq (Rust) / pyzmq (Python)

**Rust parent → Python child:**
```rust
// Rust sends a call message
let message = Message::Call {
    id: Uuid::new_v4().to_string(),
    function: "add".to_string(),
    args: vec![json!(1), json!(2)],
    namespace: "math".to_string(),
    client_name: None,
};
let bytes = to_vec(&message)?;
socket.send_multipart(vec![vec![], bytes], 0).await?;
```

**Python child receives:**
```python
# Python receives the message
parts = socket.recv_multipart()
# parts[0] = b'' (delimiter)
# parts[1] = msgpack bytes

message = msgpack.unpackb(parts[1])
# message = {'id': 'uuid', 'function': 'add', 'args': [1, 2], ...}

# Python processes and sends response
response = {'id': message['id'], 'result': 3}
socket.send_multipart([b'', msgpack.packb(response)])
```

### Q: How do I pass data between Rust and Python?

**A:** Use serde_json values for flexible data transfer:

```rust
// Rust sends complex data
use serde_json::Value;

let complex_data = Value::Array(vec![
    Value::Object({
        let mut m = HashMap::new();
        m.insert("name".to_string(), "John".into());
        m.insert("age".to_string(), 30.into());
        m
    }),
    Value::Array(vec![1, 2, 3, 4, 5]),
]);

worker.call("process_data", vec![complex_data], None).await?;
```

**Python receives and processes:**
```python
# Python receives complex data
data = message['args'][0]
# data = [{'name': 'John', 'age': 30}, [1, 2, 3, 4, 5]]

# Python processes and sends response
response = process_data(data)
socket.send_multipart([b'', msgpack.packb({
    'id': message['id'],
    'result': response
})])
```

### Q: How do I handle errors across language boundaries?

**A:** Use standardized error messages:

**Rust sends error:**
```rust
let error_message = Message::Error {
    id: request_id.to_string(),
    error: "Division by zero".to_string(),
};
socket.send_multipart(vec![vec![], to_vec(&error_message)?], 0).await?;
```

**Python receives error:**
```python
message = msgpack.unpackb(payload)

if message['type'] == 'error':
    error = message['error']
    raise RuntimeError(f"Worker error: {error}")
```

### Q: What data types are supported?

**A:** Both Rust and Python support:

- **Primitives**: integers, floats, booleans, strings
- **Collections**: arrays, objects (dicts)
- **Nested structures**: Arrays of objects, objects with arrays
- **Special**: null, binary data

**Type mapping:**
| Rust Type | Python Type | Note |
|-----------|-------------|------|
| `i64` | `int` | 64-bit integer |
| `u64` | `int` | 64-bit unsigned integer |
| `f64` | `float` | 64-bit float |
| `bool` | `bool` | Boolean |
| `String` | `str` | String |
| `Vec<T>` | `list` | Array |
| `HashMap<K,V>` | `dict` | Object |
| `Option<T>` | `None` / `T` | Optional |

### Q: How do I handle async methods in Python called from Rust?

**A:** Currently, Rust workers only support sync methods. For async Python methods, use an async wrapper:

**Python async wrapper:**
```python
import asyncio

async def async_add(a: int, b: int) -> int:
    await asyncio.sleep(0.1)  # Simulate async work
    return a + b

def sync_add(a: int, b: int) -> int:
    # Run async function in event loop
    loop = asyncio.get_event_loop()
    return loop.run_until_complete(async_add(a, b))
```

**Rust calls sync wrapper:**
```rust
let result: i64 = worker.call!("sync_add(10, 20)").await?;
```

### Q: How do I optimize cross-language performance?

**A:** Several strategies:

1. **Minimize serialization overhead**: Use primitive types when possible
2. **Batch requests**: Send multiple calls in one message
3. **Reuse connections**: Don't reconnect repeatedly
4. **Use efficient data structures**: Avoid deeply nested structures

```rust
// Efficient: simple primitive types
worker.call!("add(10, 20)").await?;

// Inefficient: complex nested structures
let complex_data = vec![
    vec![vec![1, 2, 3], vec![4, 5, 6]],
    vec![vec![7, 8, 9], vec![10, 11, 12]],
];
worker.call!("process_complex", vec![complex_data], None).await?;
```

### Q: How do I handle large data transfers?

**A:** For large data:

1. **Use streaming**: Stream data in chunks
2. **Use temporary files**: Write to disk, transfer file path
3. **Use compression**: Compress data before sending

```rust
// Example: large file transfer
let file_path = "/path/to/large/file.bin";
let file_size = std::fs::metadata(file_path)?.len();

worker.call!("transfer_file", vec![file_path.into()], None).await?;

// Python receives file path and transfers the file
```

---

## Additional Resources

### Q: Where can I find more examples?

**A:** Check the examples directory:
```bash
cd rust
cargo run --example spawn
cargo run --example connect
cargo run --example math_worker
```

### Q: Where can I find the architecture documentation?

**A:** See `rust/docs/arch.md` for detailed architecture information.

### Q: How do I contribute to the project?

**A:** Fork the repository, create a feature branch, and submit a pull request.

### Q: What is the license?

**A:** The Rust implementation is licensed under the MIT License. See `rust/LICENSE` for details.

---

**Last Updated:** 2026-02-12
**Rust Version:** 1.70.0+
**Multifrost Version:** 0.1.0
