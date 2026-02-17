# Rust Multifrost Architecture

This document describes how the Rust implementation of Multifrost navigates the language-agnostic specification defined in `docs/arch.md`.

## Overview

The Rust implementation provides a type-safe, async-native IPC library using **tokio** as the async runtime and **zmq** for ZeroMQ communication. It follows the spec's wire protocol while leveraging Rust's ownership system, type system, and concurrency primitives for safety and performance.

```
┌─────────────────────────────────────────────────────────────┐
│                     Rust Implementation                      │
├─────────────────────────────────────────────────────────────┤
│  ParentWorker (DEALER)          ChildWorker (ROUTER)        │
│  ┌──────────────────┐           ┌──────────────────┐       │
│  │ - Arc<RwLock<>>  │           │ - Arc<RwLock<>>  │       │
│  │ - tokio::spawn   │           │ - tokio::spawn   │       │
│  │ - Metrics        │           │ - MethodRegistry │       │
│  │ - CircuitBreaker │           │ - Heartbeat      │       │
│  └──────────────────┘           └──────────────────┘       │
│         │                                │                  │
│         └──────── ZeroMQ over TCP ───────┘                  │
│                  msgpack (rmp-serde)                        │
└─────────────────────────────────────────────────────────────┘
```

## Core Design Principles

### 1. Ownership and Concurrency

The Rust implementation uses **`Arc<RwLock<T>>`** for shared state across async tasks:

```rust
// Shared state pattern
struct ParentWorker {
    state: Arc<RwLock<ParentState>>,
    socket: Arc<DealerSocket>,
    metrics: Arc<Metrics>,
    logger: Arc<StructuredLogger>,
}
```

**Why this pattern?**
- **Arc**: Enables multiple ownership across async tasks (tokio::spawn)
- **RwLock**: Allows concurrent reads, exclusive writes (async-safe)
- **Thread-safe**: All shared state is `Send + Sync`
- **No data races**: Rust's type system guarantees safety

### 2. Async-Native with Tokio

All I/O operations are async using tokio:

```rust
// Non-blocking socket operations
socket.send(message, 0).await?;
let parts = socket.recv_multipart(0).await?;

// Non-blocking sleep for heartbeats
tokio::time::sleep(Duration::from_secs(interval)).await;
```

**Benefits:**
- Efficient resource usage (no thread-per-connection)
- Cooperative multitasking
- Cancellation support via tokio::select!
- Timeout handling with tokio::time::timeout

### 3. Type-Safe Message Protocol

Messages are strongly-typed using Rust enums and serde:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Call {
        id: String,
        function: String,
        args: Vec<Value>,
        namespace: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        client_name: Option<String>,
    },
    Response {
        id: String,
        result: Value,
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
```

**Advantages:**
- Compile-time type checking
- Exhaustive pattern matching
- Serialization/deserialization via serde
- Impossible to construct invalid messages

## Module Architecture

### `src/message.rs` - Message Protocol

**Responsibilities:**
- Define message types as Rust enums
- Implement serde serialization/deserialization
- Provide factory methods for common messages
- Handle msgpack timestamp encoding

**Key Implementation Details:**

```rust
// Factory methods for type-safe message creation
impl Message {
    pub fn create_stdout(output: String) -> Self {
        Self::Stdout { output }
    }

    pub fn create_heartbeat() -> Self {
        Self::Heartbeat {
            id: Uuid::new_v4().to_string(),
        }
    }
}

// Default trait for convenience
impl Default for Message {
    fn default() -> Self {
        Self::Heartbeat {
            id: Uuid::new_v4().to_string(),
        }
    }
}
```

**Timestamp Handling:**
- Uses `rmp_serde` for msgpack encoding
- Handles both integer and float timestamps (spec allows both)
- Converts to `f64` for consistency

### `src/error.rs` - Error Types

**Responsibilities:**
- Define error types using `thiserror`
- Provide context-rich error messages
- Implement `From` traits for error conversion

**Error Hierarchy:**

```rust
#[derive(Debug, thiserror::Error)]
pub enum ComlinkError {
    #[error("ZeroMQ error: {0}")]
    Zmq(#[from] zmq::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] rmp_serde::encode::Error),

    #[error("Deserialization error: {0}")]
    Deserialization(#[from] rmp_serde::decode::Error),

    #[error("Circuit breaker is open after {0} consecutive failures")]
    CircuitOpenError(usize),

    #[error("Timeout waiting for response: {0}")]
    TimeoutError(String),

    #[error("Invalid port: {port}. Must be between 1024 and 65535")]
    InvalidPortError { port: u16 },

    #[error("Function not found: {0}")]
    FunctionNotFoundError(String),

    #[error("Cannot call private method: {0}")]
    PrivateMethodError(String),
}
```

**Benefits:**
- Structured error handling
- Automatic error context propagation
- Easy to debug with source chains
- Compatible with `?` operator

### `src/child.rs` - ChildWorker Implementation

**Responsibilities:**
- Create ROUTER socket for multi-parent support
- Register methods via trait-based system
- Handle incoming requests and send responses
- Manage heartbeat responses
- Forward stdout/stderr to parents

**Key Design Patterns:**

#### 1. Trait-Based Method Registration

```rust
pub trait WorkerMethods: Send + Sync {
    fn list_functions(&self) -> Vec<String>;
    fn call_method(&self, name: &str, args: &[Value]) -> Result<Value, ComlinkError>;
}

// Users implement this trait
struct MathWorker;

impl WorkerMethods for MathWorker {
    fn list_functions(&self) -> Vec<String> {
        vec!["add".to_string(), "multiply".to_string()]
    }

    fn call_method(&self, name: &str, args: &[Value]) -> Result<Value, ComlinkError> {
        match name {
            "add" => {
                let a: i64 = args[0].as_i64().unwrap();
                let b: i64 = args[1].as_i64().unwrap();
                Ok((a + b).into())
            }
            _ => Err(ComlinkError::FunctionNotFoundError(name.to_string())),
        }
    }
}
```

**Why traits?**
- Type-safe method dispatch
- Compile-time checking
- No runtime reflection needed
- Easy to test

#### 2. ROUTER Socket Multipart Framing

```rust
// Receive: [sender_id, empty, message]
let parts = socket.recv_multipart(0).await?;
if parts.len() != 3 {
    return Err(ComlinkError::InvalidMessageFormat);
}
let sender_id = parts[0].clone();
let message_bytes = &parts[2];

// Send: [sender_id, empty, message]
socket.send_multipart(vec![
    sender_id,
    vec![],
    response_bytes,
], 0).await?;
```

**Spec Compliance:**
- ROUTER receives 3-part messages (sender_id, delimiter, payload)
- ROUTER sends back 3-part messages to route to correct parent
- Supports multiple parents simultaneously

#### 3. Port Validation

```rust
fn validate_port(port: u16) -> Result<(), ComlinkError> {
    if port < 1024 || port > 65535 {
        return Err(ComlinkError::InvalidPortError { port });
    }
    Ok(())
}
```

**Spec Requirement:**
- Must validate port range (1024-65535)
- Exit immediately if invalid
- Clear error message

#### 4. Heartbeat Response Handling

```rust
if let Message::Heartbeat { id } = message {
    let response = Message::create_heartbeat_response(&id);
    let response_bytes = serialize_message(&response)?;
    socket.send_multipart(vec![sender_id, vec![], response_bytes], 0).await?;

    // Calculate RTT
    let rtt_ms = start_time.elapsed().as_millis() as f64;
    // Store RTT for metrics
}
```

**Spec Compliance:**
- Responds to heartbeat requests
- Calculates round-trip time
- Includes RTT in metrics

### `src/parent.rs` - ParentWorker Implementation

**Responsibilities:**
- Create DEALER socket for communication
- Spawn child process in spawn mode
- Track pending requests with UUIDs
- Implement circuit breaker pattern
- Monitor heartbeats
- Collect metrics
- Log structured events

**Key Design Patterns:**

#### 1. DEALER Socket Multipart Framing

```rust
// Send: [empty, message]
socket.send_multipart(vec![
    vec![],
    message_bytes,
], 0).await?;

// Receive: [empty, message]
let parts = socket.recv_multipart(0).await?;
if parts.len() != 2 {
    return Err(ComlinkError::InvalidMessageFormat);
}
let message_bytes = &parts[1];
```

**Spec Compliance:**
- DEALER sends 2-part messages (delimiter, payload)
- DEALER receives 2-part messages
- Simpler than ROUTER (no routing IDs)

#### 2. Pending Request Tracking

```rust
struct PendingRequest {
    tx: tokio::sync::oneshot::Sender<Result<Value, ComlinkError>>,
    start_time: Instant,
}

struct ParentState {
    pending_requests: HashMap<String, PendingRequest>,
    // ... other fields
}

// When sending a call
let (tx, rx) = tokio::sync::oneshot::channel();
state.pending_requests.insert(id.clone(), PendingRequest {
    tx,
    start_time: Instant::now(),
});

// When receiving a response
if let Some(pending) = state.pending_requests.remove(&id) {
    let _ = pending.tx.send(Ok(result));
}
```

**Why oneshot channels?**
- One-to-one communication (one request, one response)
- Type-safe result passing
- Automatic cleanup when channel is dropped
- Supports timeout via tokio::time::timeout

#### 3. Circuit Breaker Pattern

```rust
struct CircuitBreaker {
    consecutive_failures: usize,
    threshold: usize,
    state: CircuitState,
    last_failure_time: Option<Instant>,
}

enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

// On error
state.consecutive_failures += 1;
if state.consecutive_failures >= state.threshold {
    state.state = CircuitState::Open;
    state.last_failure_time = Some(Instant::now());
    logger.log(LogEntry::circuit_open(state.consecutive_failures));
}

// On success
state.consecutive_failures = 0;
if state.state == CircuitState::HalfOpen {
    state.state = CircuitState::Closed;
    logger.log(LogEntry::circuit_reset());
}
```

**Spec Compliance:**
- Tracks consecutive failures (default: 5)
- Opens circuit after threshold
- Half-open state for testing recovery
- Logs circuit events

#### 4. Heartbeat Monitoring Loop

```rust
async fn heartbeat_loop(
    socket: Arc<DealerSocket>,
    state: Arc<RwLock<ParentState>>,
    interval: f64,
    timeout: f64,
    max_misses: usize,
) {
    let mut interval_timer = tokio::time::interval(Duration::from_secs_f64(interval));

    loop {
        interval_timer.tick().await;

        let heartbeat = Message::create_heartbeat();
        let heartbeat_bytes = serialize_message(&heartbeat).unwrap();

        // Send heartbeat
        if let Err(e) = socket.send_multipart(vec![vec![], heartbeat_bytes], 0).await {
            // Handle send error
            continue;
        }

        // Wait for response with timeout
        match tokio::time::timeout(
            Duration::from_secs_f64(timeout),
            wait_for_heartbeat_response(&heartbeat.id, &state)
        ).await {
            Ok(Ok(rtt_ms)) => {
                // Success - update metrics
                state.write().await.consecutive_misses = 0;
                metrics.record_heartbeat_rtt(rtt_ms);
            }
            Ok(Err(_)) | Err(_) => {
                // Timeout or error
                state.write().await.consecutive_misses += 1;
                if state.read().await.consecutive_misses >= max_misses {
                    // Circuit breaker trip
                }
            }
        }
    }
}
```

**Spec Compliance:**
- Configurable interval (default: 5.0s)
- Configurable timeout (default: 3.0s)
- Configurable max misses (default: 3)
- Tracks RTT for metrics
- Trips circuit breaker on too many misses

#### 5. Metrics Collection

```rust
struct Metrics {
    inner: Arc<RwLock<MetricsInner>>,
}

struct MetricsInner {
    request_latencies: VecDeque<f64>,
    error_count: usize,
    success_count: usize,
    circuit_breaker_events: Vec<CircuitEvent>,
    heartbeat_rtts: VecDeque<f64>,
}

// Recording a request
impl Metrics {
    pub fn start_request(&self) -> RequestHandle {
        RequestHandle {
            start_time: Instant::now(),
            metrics: Arc::clone(&self.inner),
        }
    }
}

// When request completes
impl Drop for RequestHandle {
    fn drop(&mut self) {
        let latency_ms = self.start_time.elapsed().as_millis() as f64;
        if let Ok(mut inner) = self.metrics.write() {
            inner.request_latencies.push_back(latency_ms);
            if inner.request_latencies.len() > 1000 {
                inner.request_latencies.pop_front();
            }
        }
    }
}
```

**Spec Compliance:**
- Tracks request latency (avg, p50, p95, p99, min, max)
- Tracks error rates
- Tracks circuit breaker events
- Tracks heartbeat RTT
- Thread-safe via Arc<RwLock<>>

#### 6. Structured Logging

```rust
pub struct StructuredLogger {
    handlers: Vec<Arc<dyn LogHandler>>,
}

pub trait LogHandler: Send + Sync {
    fn handle(&self, entry: &LogEntry);
}

pub struct LogEntry {
    timestamp: DateTime<Utc>,
    level: LogLevel,
    event: LogEvent,
    metadata: HashMap<String, Value>,
}

// Convenience methods
impl StructuredLogger {
    pub fn log_worker_start(&self, service_id: &str, port: u16) {
        self.log(LogEntry {
            timestamp: Utc::now(),
            level: LogLevel::Info,
            event: LogEvent::WorkerStart,
            metadata: {
                let mut m = HashMap::new();
                m.insert("service_id".to_string(), service_id.into());
                m.insert("port".to_string(), port.into());
                m
            },
        });
    }
}
```

**Spec Compliance:**
- Structured log entries with metadata
- Multiple handlers (JSON, pretty, custom)
- Log levels (Debug, Info, Warn, Error)
- Log events (WorkerStart, WorkerStop, RequestStart, etc.)
- Thread-safe via Arc

#### 7. Spawn Mode Process Management

```rust
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
    let heartbeat_handle = tokio::spawn(heartbeat_loop(
        Arc::clone(&socket),
        Arc::clone(&state),
        options.heartbeat_interval,
        options.heartbeat_timeout,
        options.heartbeat_max_misses,
    ));

    Ok(Self {
        socket: Arc::new(socket),
        state,
        child: Some(child),
        heartbeat_handle: Some(heartbeat_handle),
        // ...
    })
}
```

**Spec Compliance:**
- Finds free port
- Binds DEALER before spawning
- Sets COMLINK_ZMQ_PORT env var
- Waits for child connection
- Starts heartbeat monitoring
- Manages child process lifecycle

### `src/metrics.rs` - Metrics Collection

**Responsibilities:**
- Track request latency statistics
- Track error rates
- Track circuit breaker events
- Track heartbeat RTT
- Provide snapshots for monitoring

**Key Implementation:**

```rust
pub struct Metrics {
    inner: Arc<RwLock<MetricsInner>>,
}

pub struct MetricsInner {
    request_latencies: VecDeque<f64>,
    error_count: usize,
    success_count: usize,
    circuit_breaker_events: Vec<CircuitEvent>,
    heartbeat_rtts: VecDeque<f64>,
}

impl Metrics {
    pub fn snapshot(&self) -> MetricsSnapshot {
        let inner = self.inner.read().unwrap();
        MetricsSnapshot {
            request_metrics: self.calculate_request_metrics(&inner.request_latencies),
            error_rate: if inner.success_count + inner.error_count > 0 {
                inner.error_count as f64 / (inner.success_count + inner.error_count) as f64
            } else {
                0.0
            },
            circuit_breaker_events: inner.circuit_breaker_events.clone(),
            heartbeat_rtt_ms: self.calculate_rtt_metrics(&inner.heartbeat_rtts),
        }
    }

    fn calculate_request_metrics(&self, latencies: &VecDeque<f64>) -> RequestMetrics {
        if latencies.is_empty() {
            return RequestMetrics::default();
        }

        let mut sorted: Vec<_> = latencies.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        RequestMetrics {
            avg: latencies.iter().sum::<f64>() / latencies.len() as f64,
            min: *sorted.first().unwrap(),
            max: *sorted.last().unwrap(),
            p50: percentile(&sorted, 50.0),
            p95: percentile(&sorted, 95.0),
            p99: percentile(&sorted, 99.0),
        }
    }
}
```

**Design Decisions:**
- **VecDeque**: Efficient circular buffer for sliding window
- **Arc<RwLock<>>**: Thread-safe access
- **Snapshot pattern**: Immutable snapshots for monitoring
- **Percentile calculation**: Sorted array for accurate percentiles

### `src/logging.rs` - Structured Logging

**Responsibilities:**
- Provide structured logging interface
- Support multiple log handlers
- Define log levels and events
- Provide convenience methods

**Key Implementation:**

```rust
pub struct StructuredLogger {
    handlers: Vec<Arc<dyn LogHandler>>,
}

impl Clone for StructuredLogger {
    fn clone(&self) -> Self {
        Self {
            handlers: self.handlers.iter().map(|h| Arc::clone(h)).collect(),
        }
    }
}

pub trait LogHandler: Send + Sync {
    fn handle(&self, entry: &LogEntry);
}

// Default JSON handler
pub fn default_json_handler() -> Arc<dyn LogHandler> {
    Arc::new(|entry: &LogEntry| {
        let json = serde_json::to_string(entry).unwrap();
        println!("{}", json);
    })
}

// Default pretty handler
pub fn default_pretty_handler() -> Arc<dyn LogHandler> {
    Arc::new(|entry: &LogEntry| {
        println!("[{}] {}: {}",
            entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
            entry.level,
            entry.event
        );
    })
}
```

**Design Decisions:**
- **Arc<dyn LogHandler>**: Trait object for flexible handlers
- **Clone trait**: Allows cloning logger across tasks
- **Default handlers**: JSON and pretty-printed formats
- **Convenience methods**: Type-safe event logging

### `src/registry.rs` - Service Registry

**Responsibilities:**
- Register services in connect mode
- Discover services by service_id
- Unregister services on shutdown
- Handle PID validation

**Current Implementation:**
- Basic JSON file storage
- PID validation
- Service discovery

**Future Enhancements:**
- File locking (O_CREAT | O_EXCL pattern)
- Lock timeout handling
- Atomic updates

## Type System Usage

### 1. Enum for Message Types

```rust
pub enum Message {
    Call { ... },
    Response { ... },
    Error { ... },
    Stdout { ... },
    Stderr { ... },
    Heartbeat { ... },
}
```

**Benefits:**
- Exhaustive pattern matching
- Impossible to have invalid message type
- Compiler enforces handling all cases

### 2. Result<T, E> for Error Handling

```rust
pub fn call_method(&self, name: &str, args: &[Value]) -> Result<Value, ComlinkError> {
    match name {
        "add" => Ok(...),
        _ => Err(ComlinkError::FunctionNotFoundError(name.to_string())),
    }
}
```

**Benefits:**
- Explicit error handling
- Type-safe error propagation
- No exceptions

### 3. Option<T> for Optional Values

```rust
pub struct CallMessage {
    pub client_name: Option<String>,
}
```

**Benefits:**
- Explicit nullability
- No null pointer exceptions
- Compiler-checked usage

## Async Runtime Integration

### Tokio as Async Runtime

The Rust implementation uses **tokio** as the async runtime:

```rust
#[tokio::main]
async fn main() -> Result<(), ComlinkError> {
    let worker = ParentWorker::spawn("./math_worker.py", None, SpawnOptions::default()).await?;
    worker.start().await?;

    let result = worker.call("add", &[1.into(), 2.into()], None).await?;

    worker.close().await?;
    Ok(())
}
```

**Why tokio?**
- Industry-standard async runtime for Rust
- Excellent ZeroMQ integration
- Comprehensive async primitives
- Efficient task scheduling

### Async Task Spawning

```rust
// Spawn heartbeat monitoring task
let heartbeat_handle = tokio::spawn(heartbeat_loop(
    Arc::clone(&socket),
    Arc::clone(&state),
    interval,
    timeout,
    max_misses,
));

// Spawn message receiving task
let receive_handle = tokio::spawn(receive_loop(
    Arc::clone(&socket),
    Arc::clone(&state),
));
```

**Benefits:**
- Concurrent execution without threads
- Efficient resource usage
- Cancellation support

### Timeout Handling

```rust
// Timeout on request
match tokio::time::timeout(
    Duration::from_secs(timeout),
    wait_for_response(&id, &state)
).await {
    Ok(result) => result,
    Err(_) => Err(ComlinkError::TimeoutError(id)),
}
```

**Benefits:**
- Prevents hanging requests
- Clean error handling
- Configurable timeouts

## Memory Management

### Arc for Shared Ownership

```rust
struct ParentWorker {
    socket: Arc<DealerSocket>,
    state: Arc<RwLock<ParentState>>,
    metrics: Arc<Metrics>,
    logger: Arc<StructuredLogger>,
}
```

**Why Arc?**
- Enables multiple ownership across tasks
- Reference counting for automatic cleanup
- Thread-safe (Send + Sync)

### RwLock for Mutable Shared State

```rust
struct ParentState {
    pending_requests: HashMap<String, PendingRequest>,
    consecutive_failures: usize,
    circuit_state: CircuitState,
    // ...
}

// Read access
let state = self.state.read().await?;
let is_healthy = state.consecutive_failures < state.threshold;

// Write access
let mut state = self.state.write().await?;
state.consecutive_failures += 1;
```

**Why RwLock?**
- Allows concurrent reads
- Exclusive writes
- Async-compatible (tokio::sync::RwLock)

### VecDeque for Circular Buffers

```rust
struct MetricsInner {
    request_latencies: VecDeque<f64>,
    heartbeat_rtts: VecDeque<f64>,
}

// Add new value
inner.request_latencies.push_back(latency);

// Remove old value if buffer full
if inner.request_latencies.len() > 1000 {
    inner.request_latencies.pop_front();
}
```

**Why VecDeque?**
- Efficient push/pop from both ends
- O(1) operations
- Perfect for sliding windows

## ZeroMQ Integration

### Socket Types

```rust
use zmq::{Context, DealerSocket, RouterSocket};

// Parent uses DEALER
let socket = DealerSocket::bind("tcp://*:5555")?;

// Child uses ROUTER
let socket = RouterSocket::connect("tcp://localhost:5555")?;
```

**Spec Compliance:**
- Parent: DEALER (initiates communication)
- Child: ROUTER (handles multiple parents)

### Multipart Message Handling

```rust
// Send multipart
socket.send_multipart(vec![
    vec![],  // delimiter
    message_bytes,
], 0).await?;

// Receive multipart
let parts = socket.recv_multipart(0).await?;
let delimiter = &parts[0];
let message_bytes = &parts[1];
```

**Spec Compliance:**
- DEALER: 2-part messages (delimiter, payload)
- ROUTER: 3-part messages (sender_id, delimiter, payload)

### Socket Options

```rust
// Default options (as per spec)
socket.set_linger(0)?;  // Don't wait on close
socket.set_sndtimeo(5000)?;  // 5s send timeout
socket.set_rcvtimeo(5000)?;  // 5s receive timeout
```

**Spec Compliance:**
- Uses default socket options
- Configurable timeouts
- Clean shutdown

## msgpack Serialization

### Using rmp-serde

```rust
use serde::{Serialize, Deserialize};
use rmp_serde::{to_vec, from_slice};

// Serialize
let message = Message::Call { ... };
let bytes = to_vec(&message)?;

// Deserialize
let message: Message = from_slice(&bytes)?;
```

**Why rmp-serde?**
- serde integration (type-safe)
- Efficient binary encoding
- Compatible with Python msgpack

### Timestamp Handling

```rust
// Convert to f64 for consistency
let timestamp = Utc::now().timestamp() as f64;

// Deserialize handles both int and float
#[derive(Deserialize)]
struct Message {
    #[serde(deserialize_with = "deserialize_timestamp")]
    timestamp: f64,
}
```

**Spec Compliance:**
- Handles both integer and float timestamps
- Converts to f64 for consistency
- Compatible with Python implementation

## Error Handling Strategy

### thiserror for Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ComlinkError {
    #[error("ZeroMQ error: {0}")]
    Zmq(#[from] zmq::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] rmp_serde::encode::Error),

    #[error("Function not found: {0}")]
    FunctionNotFoundError(String),
}
```

**Benefits:**
- Automatic error context
- Easy error conversion
- Clean error messages

### ? Operator for Propagation

```rust
pub async fn call(&self, function: &str, args: &[Value]) -> Result<Value, ComlinkError> {
    let message = Message::Call { ... };
    let bytes = to_vec(&message)?;  // Propagates serialization errors
    self.socket.send_multipart(...).await?;  // Propagates ZMQ errors
    // ...
}
```

**Benefits:**
- Concise error handling
- Automatic error conversion
- Type-safe

### Context-Rich Errors

```rust
#[error("Invalid port: {port}. Must be between 1024 and 65535")]
InvalidPortError { port: u16 },
```

**Benefits:**
- Self-documenting errors
- Includes relevant context
- Easy to debug

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_port_valid() {
        assert!(validate_port(8080).is_ok());
    }

    #[test]
    fn test_validate_port_invalid() {
        assert!(validate_port(80).is_err());
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_parent_child_communication() {
    let child = ChildWorker::new("test-service");
    tokio::spawn(async move {
        child.run().await;
    });

    let parent = ParentWorker::connect("test-service").await?;
    let result = parent.call("add", &[1.into(), 2.into()], None).await?;
    assert_eq!(result, 3);
}
```

## Performance Considerations

### 1. Zero-Copy Where Possible

```rust
// Avoid unnecessary copies
let parts = socket.recv_multipart(0).await?;
let message_bytes = &parts[1];  // Reference, not copy
```

### 2. Efficient Data Structures

```rust
// HashMap for O(1) lookups
let pending_requests: HashMap<String, PendingRequest> = ...;

// VecDeque for O(1) push/pop
let latencies: VecDeque<f64> = ...;
```

### 3. Async Non-Blocking

```rust
// All I/O is async
socket.send_multipart(...).await?;
socket.recv_multipart(0).await?;
```

### 4. Bounded Buffers

```rust
// Limit buffer size to prevent memory bloat
if latencies.len() > 1000 {
    latencies.pop_front();
}
```

## Security Considerations

### 1. Input Validation

```rust
// Validate port range
if port < 1024 || port > 65535 {
    return Err(ComlinkError::InvalidPortError { port });
}

// Validate function names
if function.starts_with('_') {
    return Err(ComlinkError::PrivateMethodError(function.to_string()));
}
```

### 2. No Unsafe Code

The implementation uses **zero unsafe code**:
- All memory safety guaranteed by Rust's type system
- No manual memory management
- No null pointer dereferences
- No data races

### 3. Localhost Only

```rust
// Bind to localhost only
let socket = DealerSocket::bind("tcp://127.0.0.1:5555")?;
```

**Spec Compliance:**
- Only use on localhost or trusted networks
- Do not expose ports to untrusted clients

## Comparison with Python Implementation

| Feature | Python | Rust |
|---------|--------|------|
| Async Runtime | asyncio | tokio |
| Shared State | threading.Lock | Arc<RwLock<>> |
| Message Types | Dict | Enum |
| Serialization | msgpack | rmp-serde |
| Error Handling | Exceptions | Result<T, E> |
| Type Safety | Runtime | Compile-time |
| Memory Safety | GC | Ownership |
| Zero-Copy | No | Yes (partial) |
| Unsafe Code | No | No |
| Handle Pattern | Yes (async + sync) | Yes (async only) |

## Worker → Handle Pattern (v4)

Starting with v4, the API separates process definition (Worker) from runtime interface (Handle):

```rust
// v4 recommended pattern
let worker = ParentWorker::spawn("./worker", None, SpawnOptions::default()).await?;
let handle = worker.handle();

handle.start().await?;
let result = handle.call("add", &[1.into(), 2.into()], None).await?;
handle.stop().await?;
```

**Key concepts:**
- **Worker** = config/state (holds socket, process, registry internally)
- **Handle** = lightweight API view (call interface only)
- Handle is cheap to create — multiple handles from same worker is fine
- Rust's ownership model: `start()`/`stop()` must be called directly on worker (requires mutable access)

**Rust-specific note:**

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

### Handle Struct

```rust
pub struct Handle<'a> {
    worker: &'a ParentWorker,
}

impl Handle<'_> {
    /// Call a remote method
    pub async fn call(&self, function: &str, args: Vec<Value>, options: Option<CallOptions>) -> Result<Value, ComlinkError>;
    
    /// Call with raw values
    pub async fn call_raw(&self, function: &str, args: Vec<Value>) -> Result<Value, ComlinkError>;
    
    /// Call with timeout
    pub async fn call_with_timeout(&self, function: &str, args: Vec<Value>, timeout: Duration) -> Result<Value, ComlinkError>;
    
    /// Check if worker is healthy
    pub fn is_healthy(&self) -> bool;
    
    /// Check if circuit breaker is open
    pub fn circuit_open(&self) -> bool;
    
    /// Get metrics snapshot
    pub fn metrics(&self) -> MetricsSnapshot;
}
```

## Future Enhancements

### 1. File Locking for Registry

Implement atomic file locking using `O_CREAT | O_EXCL` pattern:

```rust
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;

let lock_file = OpenOptions::new()
    .write(true)
    .create_new(true)  // O_CREAT | O_EXCL
    .open("/tmp/registry.lock")?;
```

### 2. Async Method Handlers

Support async method handlers in ChildWorker:

```rust
pub trait AsyncWorkerMethods: Send + Sync {
    async fn call_method_async(&self, name: &str, args: &[Value]) -> Result<Value, ComlinkError>;
}
```

### 3. Connection Pooling

Support multiple child workers with load balancing:

```rust
struct WorkerPool {
    workers: Vec<ParentWorker>,
    strategy: LoadBalancingStrategy,
}
```

### 4. TLS Encryption

Add TLS support for encrypted communication:

```rust
use tokio_rustls::TlsConnector;

let connector = TlsConnector::from(Arc::new(config));
let stream = connector.connect("localhost", socket).await?;
```

## Conclusion

The Rust implementation of Multifrost provides a type-safe, async-native IPC library that fully complies with the language-agnostic specification while leveraging Rust's unique features:

- **Ownership system** for memory safety without GC
- **Type system** for compile-time guarantees
- **Arc<RwLock<>>** for safe shared state
- **tokio** for efficient async I/O
- **serde** for type-safe serialization
- **thiserror** for structured error handling

The result is a robust, performant, and maintainable implementation that's ready for production use.
