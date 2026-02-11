//! Multifrost - Lightweight IPC library for Rust
//!
//! Spawn and control worker processes through async calls.
//! Compatible with Python, JavaScript, and Go versions.
//!
//! # Example
//!
//! ## Child Worker
//! ```rust,no_run
//! use multifrost::{ChildWorker, ChildWorkerContext, run_worker, Result};
//! use async_trait::async_trait;
//! use serde_json::Value;
//!
//! struct MathWorker;
//!
//! #[async_trait]
//! impl ChildWorker for MathWorker {
//!     async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
//!         match function {
//!             "add" => {
//!                 let a = args[0].as_i64().unwrap();
//!                 let b = args[1].as_i64().unwrap();
//!                 Ok(serde_json::json!(a + b))
//!             }
//!             _ => Err(multifrost::MultifrostError::FunctionNotFound(function.to_string()))
//!         }
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     run_worker(MathWorker, ChildWorkerContext::new()).await;
//! }
//! ```
//!
//! ## Parent (with ergonomic API)
//! ```rust,no_run
//! use multifrost::{ParentWorker, call};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut worker = ParentWorker::spawn("examples/math_worker.rs", "cargo run --example math_worker").await?;
//!     worker.start().await?;
//!
//!     // Using the ergonomic call! macro
//!     let result: i64 = worker.call!(add(10, 20)).await?;
//!     println!("10 + 20 = {}", result);
//!
//!     worker.stop().await;
//!     Ok(())
//! }
//! ```
//!
//! ## Parent (with builder pattern)
//! ```rust,no_run
//! use multifrost::{ParentWorkerBuilder, ParentWorker};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let worker = ParentWorkerBuilder::spawn("examples/math_worker.rs", "cargo run --example math_worker")
//!         .auto_restart(true)
//!         .default_timeout(Duration::from_secs(30))
//!         .build()
//!         .await?;
//!
//!     worker.start().await?;
//!     // ...
//!     Ok(())
//! }
//! ```

mod error;
mod message;
mod registry;
mod child;
mod parent;
mod metrics;
mod logging;
mod call_macro;

#[cfg(test)]
mod test_utils;

pub use error::{MultifrostError, Result};
pub use message::{Message, MessageType};
pub use registry::ServiceRegistry;
pub use child::{ChildWorker, ChildWorkerContext, run_worker, run_worker_sync, SyncChildWorker};
pub use parent::{ParentWorker, ParentWorkerBuilder, SpawnOptions, ConnectOptions};
pub use metrics::{Metrics, MetricsSnapshot, RequestMetrics};
pub use logging::{
    StructuredLogger, LogEntry, LogEvent, LogLevel, LogHandler,
    default_json_handler, default_pretty_handler, LogOptions,
};
pub use call_macro::{ExtractArgs, ArgsExtractor, ToJsonArg, ErgonomicCall, CallArgs};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
