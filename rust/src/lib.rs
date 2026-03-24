//! FILE: rust/src/lib.rs
//! PURPOSE: Define the crate root public API and re-export the canonical v5 surface.
//! OWNS: Root re-exports, crate documentation examples, version constant.
//! EXPORTS: connect, spawn, Connection, Handle, ServiceProcess, ServiceWorker, ServiceContext, run_service, run_service_sync.
//! DOCS: agent_chat/rust_v5_api_surface_2026-03-24.md
//!
//! Multifrost is a router-based IPC library for Rust.
//!
//! Service peers expose methods under a stable `peer_id`.
//! Caller peers connect to the router and issue typed calls.
//!
//! # Caller Example
//!
//! ```rust,no_run
//! use multifrost::{call, connect};
//!
//! # #[tokio::main]
//! # async fn main() -> multifrost::Result<()> {
//! let connection = connect("math-service", 5_000).await?;
//! let handle = connection.handle();
//! handle.start().await?;
//! let sum: i64 = call!(handle, add(10, 20)).await?;
//! assert_eq!(sum, 30);
//! handle.stop().await;
//! # Ok(())
//! # }
//! ```
//!
//! # Service Example
//!
//! ```rust,no_run
//! use async_trait::async_trait;
//! use multifrost::{run_service, MultifrostError, Result, ServiceContext, ServiceWorker};
//! use serde_json::Value;
//!
//! struct MathWorker;
//!
//! #[async_trait]
//! impl ServiceWorker for MathWorker {
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
//! run_service(MathWorker, ServiceContext::new().with_service_id("math-service")).await;
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
mod process;
mod router_bootstrap;

pub use call_macro::{ArgsExtractor, CallArgs, ErgonomicCall, ExtractArgs, ToJsonArg};
pub use child::{run_service, run_service_sync, ServiceContext, ServiceWorker, SyncServiceWorker};
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
pub use parent::{connect, ConnectOptions, Connection, Handle};
pub use process::{spawn, ServiceProcess, SpawnOptions};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
