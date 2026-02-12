# Rust Multifrost AGENTS Guide

## Overview

The Rust implementation provides a type-safe, async-native IPC library using **tokio** for async runtime and **zmq** for ZeroMQ communication. It uses Arc<RwLock<>> for shared state, trait-based method registration, and msgpack (rmp-serde) for serialization.

```
ParentWorker (DEALER)          ChildWorker (ROUTER)
┌──────────────────┐           ┌──────────────────┐
│ - Arc<RwLock<>>  │           │ - Arc<RwLock<>>  │
│ - tokio::spawn   │           │ - tokio::spawn   │
│ - Metrics        │           │ - MethodRegistry │
│ - CircuitBreaker │           │ - Heartbeat      │
└──────────────────┘           └──────────────────┘
         │                                │
         └──────── ZeroMQ over TCP ───────┘
                  msgpack (rmp-serde)
```

## Commands

```bash
# Build and test
cargo build          # Build Rust code
cargo test           # Run all tests
cargo test -- --nocapture  # Run tests with output

# Run examples
cargo run --example parent
cargo run --example child

# Format code
cargo fmt

# Check code without building
cargo clippy
```

## Code Style Guidelines

### Import Organization
- Standard library first
- Third-party crates second
- Local modules last
- Group alphabetically within each section

```rust
// Correct
use std::collections::HashMap;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use crate::message::Message;
```

### Formatting
- Use `cargo fmt` with default settings (automatic)
- No manual formatting needed

### Type Safety
- Prefer `Result<T, E>` over `Option<T>` for error handling
- Use rustc's type checking
- Avoid unnecessary pointers

```rust
// Correct
fn call_method(&self, name: &str, args: &[Value]) -> Result<Value, ComlinkError>

// Avoid
fn call_method(&self, name: &str, args: &[Value]) -> Option<Value>
```

### Naming Conventions
- `PascalCase` for exported types/functions (pub)
- `snake_case` for private functions/variables
- `SCREAMING_SNAKE_CASE` for constants

```rust
// Correct
pub struct ParentWorker;
pub fn spawn_worker() -> Result<(), ComlinkError>;

// Avoid
pub struct parentWorker;
pub fn SpawnWorker();
```

### Error Handling
- Use `thiserror` for error types
- Return `Result<T, E>` from functions
- Use `?` operator for error propagation
- Provide context-rich error messages

```rust
// Correct
#[derive(Debug, thiserror::Error)]
pub enum ComlinkError {
    #[error("ZeroMQ error: {0}")]
    Zmq(#[from] zmq::Error),
}

fn call(&self) -> Result<Value, ComlinkError> {
    self.socket.send(...).await?;  // Propagates errors
}
```

### Documentation
- Use `///` for item documentation
- Use `//!` for module documentation
- Keep it concise and focused on API usage

```rust
/// Creates a new ParentWorker by spawning a child process.
///
/// # Arguments
/// * `script_path` - Path to the child script
/// * `executable` - Optional path to executable
/// * `options` - Spawn configuration options
///
/// # Returns
/// Returns Ok(ParentWorker) on success, Err(ComlinkError) on failure
pub fn spawn(script_path: &str, executable: Option<&str>, options: SpawnOptions) -> Result<Self, ComlinkError>
```

## Architecture

### Async Runtime
- All I/O uses tokio's async primitives
- Non-blocking socket operations
- Use `tokio::time::sleep`, `tokio::time::timeout`
- Spawn background tasks with `tokio::spawn`

```rust
#[tokio::main]
async fn main() -> Result<(), ComlinkError> {
    let worker = ParentWorker::spawn("./child.py", None, SpawnOptions::default()).await?;
    worker.start().await?;
    Ok(())
}
```

### Shared State
- Use `Arc<RwLock<T>>` for shared mutable state
- Read with `.read().await`
- Write with `.write().await`

```rust
struct ParentWorker {
    state: Arc<RwLock<ParentState>>,
    socket: Arc<DealerSocket>,
}
```

### ZeroMQ Integration
- Parent uses DEALER socket (2-part messages: delimiter, payload)
- Child uses ROUTER socket (3-part messages: sender_id, delimiter, payload)
- Use `send_multipart` and `recv_multipart` methods

```rust
// DEALER: [empty, message_bytes]
socket.send_multipart(vec![vec![], message_bytes], 0).await?;

// ROUTER: [sender_id, empty, message_bytes]
let parts = socket.recv_multipart(0).await?;
```

### Serialization
- Use `rmp-serde` for msgpack encoding
- Serialize with `to_vec(&message)?`
- Deserialize with `from_slice(&bytes)?`

```rust
use rmp_serde::{to_vec, from_slice};

let bytes = to_vec(&message)?;
let message: Message = from_slice(&bytes)?;
```

### Trait-Based Method Registration
- Child implements `WorkerMethods` trait
- Methods are type-safe and compile-checked
- No runtime reflection needed

```rust
pub trait WorkerMethods: Send + Sync {
    fn list_functions(&self) -> Vec<String>;
    fn call_method(&self, name: &str, args: &[Value]) -> Result<Value, ComlinkError>;
}
```

## Testing

```rust
// Unit tests
#[cfg(test)]
mod tests {
    #[test]
    fn test_validate_port() {
        assert!(validate_port(8080).is_ok());
    }
}

// Async tests
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
