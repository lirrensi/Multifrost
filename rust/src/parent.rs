//! FILE: rust/src/parent.rs
//! PURPOSE: Own the caller connection surface for the v5 IPC runtime.
//! OWNS: ConnectOptions, Connection, ConnectionBuilder, Handle, caller transport wiring.
//! EXPORTS: ConnectOptions, Connection, Handle, connect.
//! DOCS: agent_chat/rust_v5_api_surface_2026-03-24.md

use crate::caller_transport::{CallerTransport, CallerTransportConfig};
use crate::error::Result;
use crate::message::QueryGetResponseBody;
use crate::metrics::Metrics;
use crate::process::ServiceProcess;
use crate::router_bootstrap::{router_port_from_env, RouterBootstrapConfig, RouterEndpointConfig};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct ConnectOptions {
    pub caller_peer_id: Option<String>,
    pub request_timeout: Option<Duration>,
    pub router_port: Option<u16>,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            caller_peer_id: None,
            request_timeout: Some(DEFAULT_REQUEST_TIMEOUT),
            router_port: None,
        }
    }
}

#[derive(Clone)]
struct ConnectionRuntime {
    transport: CallerTransport,
    default_target_peer_id: Option<String>,
    metrics: Option<Metrics>,
    service_process: Arc<Mutex<Option<ServiceProcess>>>,
}

#[derive(Clone)]
pub struct Connection {
    inner: Arc<ConnectionRuntime>,
}

#[derive(Clone)]
pub struct Handle {
    inner: Arc<ConnectionRuntime>,
}

pub async fn connect(default_target_peer_id: &str, timeout_ms: u64) -> Result<Connection> {
    Connection::connect(default_target_peer_id, timeout_ms).await
}

impl Connection {
    pub async fn connect(default_target_peer_id: &str, timeout_ms: u64) -> Result<Self> {
        Self::connect_with_options(
            default_target_peer_id,
            ConnectOptions {
                caller_peer_id: None,
                request_timeout: Some(Duration::from_millis(timeout_ms)),
                router_port: None,
            },
        )
        .await
    }

    pub async fn connect_with_options(
        default_target_peer_id: &str,
        options: ConnectOptions,
    ) -> Result<Self> {
        let transport = connect_caller_transport(
            default_target_peer_id.to_string(),
            options.caller_peer_id,
            options.request_timeout,
            options.router_port,
        )
        .await?;
        Ok(Self::from_transport(
            transport,
            Some(default_target_peer_id.to_string()),
        ))
    }

    fn from_transport(transport: CallerTransport, default_target_peer_id: Option<String>) -> Self {
        let runtime = ConnectionRuntime {
            transport,
            default_target_peer_id,
            metrics: Some(Metrics::new()),
            service_process: Arc::new(Mutex::new(None)),
        };
        Self {
            inner: Arc::new(runtime),
        }
    }

    pub fn with_service_process(self, service_process: ServiceProcess) -> Self {
        {
            let mut guard = self
                .inner
                .service_process
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *guard = Some(service_process);
        }
        self
    }

    pub fn handle(&self) -> Handle {
        Handle {
            inner: Arc::clone(&self.inner),
        }
    }

    pub fn peer_id(&self) -> &str {
        self.inner.transport.peer_id()
    }

    pub fn default_target_peer_id(&self) -> Option<&str> {
        self.inner.default_target_peer_id.as_deref()
    }
}

impl Handle {
    pub async fn start(&self) -> Result<()> {
        Ok(())
    }

    pub async fn stop(&self) {
        let _ = self.inner.transport.disconnect().await;
        let mut guard = self
            .inner
            .service_process
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(mut service_process) = guard.take() {
            let _ = service_process.stop();
        }
    }

    pub async fn is_healthy(&self) -> bool {
        true
    }

    pub async fn circuit_open(&self) -> bool {
        false
    }

    pub async fn last_heartbeat_rtt_ms(&self) -> Option<f64> {
        None
    }

    pub fn metrics(&self) -> Option<&Metrics> {
        self.inner.metrics.as_ref()
    }

    pub fn peer_id(&self) -> &str {
        self.inner.transport.peer_id()
    }

    pub fn default_target_peer_id(&self) -> Option<&str> {
        self.inner.default_target_peer_id.as_deref()
    }

    pub async fn call<R>(&self, function: &str, args: Vec<Value>) -> Result<R>
    where
        R: serde::de::DeserializeOwned,
    {
        self.inner.transport.call(function, args).await
    }

    pub async fn call_to<R>(
        &self,
        target_peer_id: &str,
        function: &str,
        args: Vec<Value>,
    ) -> Result<R>
    where
        R: serde::de::DeserializeOwned,
    {
        self.inner
            .transport
            .call_to(target_peer_id, function, args)
            .await
    }

    pub async fn query_peer_exists(&self, peer_id: &str) -> Result<bool> {
        Ok(self
            .inner
            .transport
            .query_peer_exists(peer_id)
            .await?
            .exists)
    }

    pub async fn query_peer_get(&self, peer_id: &str) -> Result<QueryGetResponseBody> {
        self.inner.transport.query_peer_get(peer_id).await
    }
}

#[derive(Clone)]
pub struct ConnectionBuilder {
    target_peer_id: String,
    options: BuilderOptions,
}

#[derive(Clone)]
struct BuilderOptions {
    caller_peer_id: Option<String>,
    request_timeout: Option<Duration>,
    router_port: Option<u16>,
}

impl Default for BuilderOptions {
    fn default() -> Self {
        Self {
            caller_peer_id: None,
            request_timeout: Some(DEFAULT_REQUEST_TIMEOUT),
            router_port: None,
        }
    }
}

impl ConnectionBuilder {
    pub fn connect(target_peer_id: &str) -> Self {
        Self {
            target_peer_id: target_peer_id.to_string(),
            options: BuilderOptions::default(),
        }
    }

    pub fn caller_peer_id(mut self, peer_id: &str) -> Self {
        self.options.caller_peer_id = Some(peer_id.to_string());
        self
    }

    pub fn default_timeout(mut self, timeout: Duration) -> Self {
        self.options.request_timeout = Some(timeout);
        self
    }

    pub fn router_port(mut self, port: u16) -> Self {
        self.options.router_port = Some(port);
        self
    }

    pub async fn build(self) -> Result<Connection> {
        Connection::connect_with_options(
            &self.target_peer_id,
            ConnectOptions {
                caller_peer_id: self.options.caller_peer_id,
                request_timeout: self.options.request_timeout,
                router_port: self.options.router_port,
            },
        )
        .await
    }
}

async fn connect_caller_transport(
    default_target_peer_id: String,
    caller_peer_id: Option<String>,
    request_timeout: Option<Duration>,
    router_port: Option<u16>,
) -> Result<CallerTransport> {
    let router_port = router_port.unwrap_or_else(router_port_from_env);
    let bootstrap = RouterBootstrapConfig {
        endpoint: RouterEndpointConfig {
            host: "127.0.0.1".to_string(),
            port: router_port,
        },
        lock_path: crate::router_bootstrap::default_router_lock_path(),
        log_path: crate::router_bootstrap::default_router_log_path(),
        readiness_timeout: DEFAULT_REQUEST_TIMEOUT,
        readiness_poll_interval: Duration::from_millis(100),
    };

    let peer_id = caller_peer_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let transport = CallerTransport::connect(CallerTransportConfig {
        peer_id,
        default_target_peer_id: Some(default_target_peer_id),
        request_timeout,
        bootstrap,
    })
    .await?;
    Ok(transport)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_timeout_is_present() {
        let options = crate::SpawnOptions::default();
        assert!(options.request_timeout.is_some());
    }

    #[test]
    fn builder_connect_uses_target_peer_id() {
        let builder = ConnectionBuilder::connect("math-service");
        assert_eq!(builder.target_peer_id, "math-service");
    }

    #[test]
    fn connect_options_default_to_timeout() {
        let options = ConnectOptions::default();
        assert!(options.request_timeout.is_some());
        assert!(options.router_port.is_none());
    }
}
