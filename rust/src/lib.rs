//! Multifrost is a router-based IPC library for Rust.
//!
//! Service peers expose methods under a stable `peer_id`.
//! Caller peers connect to the router and issue typed calls.
//!
//! # Caller Example
//!
//! ```rust,no_run
//! use multifrost::{call, ParentWorker};
//!
//! # #[tokio::main]
//! # async fn main() -> multifrost::Result<()> {
//! let worker = ParentWorker::connect("math-service", 5_000).await?;
//! let handle = worker.handle();
//! let sum: i64 = call!(handle, add(10, 20)).await?;
//! assert_eq!(sum, 30);
//! # Ok(())
//! # }
//! ```
//!
//! # Service Example
//!
//! ```rust,no_run
//! use async_trait::async_trait;
//! use multifrost::{ChildWorker, ChildWorkerContext, MultifrostError, Result, run_worker};
//! use serde_json::Value;
//!
//! struct MathWorker;
//!
//! #[async_trait]
//! impl ChildWorker for MathWorker {
//!     async fn handle_call(&self, function: &str, args: Vec<Value>) -> Result<Value> {
//!         match function {
//!             "add" => {
//!                 let args = multifrost::ArgsExtractor::new(args);
//!                 let a: i64 = args.get(0)?;
//!                 let b: i64 = args.get(1)?;
//!                 Ok(serde_json::json!(a + b))
//!             }
//!             _ => Err(MultifrostError::FunctionNotFound(function.to_string())),
//!         }
//!     }
//! }
//!
//! # #[tokio::main]
//! # async fn main() {
//! run_worker(MathWorker, ChildWorkerContext::new().with_service_id("math-service")).await;
//! # }
//! ```

mod call_macro;
mod caller_transport;
mod child;
mod error;
mod logging;
mod message;
mod metrics;
mod parent;
mod router_bootstrap;

pub use call_macro::{ArgsExtractor, CallArgs, ErgonomicCall, ExtractArgs, ToJsonArg};
pub use child::{run_worker, run_worker_sync, ChildWorker, ChildWorkerContext, SyncChildWorker};
pub use error::{MultifrostError, Result};
pub use logging::{
    default_json_handler, default_pretty_handler, LogEntry, LogEvent, LogHandler, LogLevel,
    LogOptions, StructuredLogger,
};
pub use message::{
    CallBody, Envelope, ErrorBody, FrameParts, PeerClass, QueryBody, QueryExistsResponseBody,
    QueryGetResponseBody, RegisterAckBody, RegisterBody, ResponseBody, WireValue,
};
pub use metrics::{Metrics, MetricsSnapshot, RequestMetrics};
pub use parent::{ConnectOptions, Handle, ParentWorker, ParentWorkerBuilder, SpawnOptions};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
