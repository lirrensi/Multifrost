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
//! ## Parent
//! ```rust,no_run
//! use multifrost::ParentWorker;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut worker = ParentWorker::spawn("examples/math_worker.rs", "cargo run --example math_worker")?;
//!     worker.start().await?;
//!
//!     let result: i64 = worker.call("add", vec![serde_json::json!(1), serde_json::json!(2)]).await?;
//!     println!("1 + 2 = {}", result);
//!
//!     worker.stop().await;
//!     Ok(())
//! }
//! ```

mod error;
mod message;
mod registry;
mod child;
mod parent;

pub use error::{MultifrostError, Result};
pub use message::{Message, MessageType};
pub use registry::ServiceRegistry;
pub use child::{ChildWorker, ChildWorkerContext, run_worker};
pub use parent::ParentWorker;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
