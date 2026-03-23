use crate::caller_transport::{CallerTransport, CallerTransportConfig};
use crate::error::{MultifrostError, Result};
use crate::message::QueryGetResponseBody;
use crate::metrics::Metrics;
use crate::router_bootstrap::{router_port_from_env, RouterBootstrapConfig, RouterEndpointConfig};
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct SpawnOptions {
    pub caller_peer_id: Option<String>,
    pub request_timeout: Option<Duration>,
    pub router_port: Option<u16>,
}

impl Default for SpawnOptions {
    fn default() -> Self {
        Self {
            caller_peer_id: None,
            request_timeout: Some(DEFAULT_REQUEST_TIMEOUT),
            router_port: None,
        }
    }
}

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
struct ParentRuntime {
    transport: CallerTransport,
    default_target_peer_id: Option<String>,
    metrics: Option<Metrics>,
    service_process: Arc<Mutex<Option<Child>>>,
}

#[derive(Clone)]
pub struct ParentWorker {
    inner: Arc<ParentRuntime>,
}

#[derive(Clone)]
pub struct Handle {
    inner: Arc<ParentRuntime>,
}

impl ParentWorker {
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
            None,
        ))
    }

    pub async fn spawn(service_entrypoint: &str, executable: &str) -> Result<Self> {
        Self::spawn_with_options(service_entrypoint, executable, SpawnOptions::default()).await
    }

    pub async fn spawn_with_options(
        service_entrypoint: &str,
        executable: &str,
        options: SpawnOptions,
    ) -> Result<Self> {
        let target_peer_id = canonical_peer_path(PathBuf::from(service_entrypoint));
        let service_process =
            spawn_service_process(executable, service_entrypoint, options.router_port)?;

        let transport = connect_caller_transport(
            target_peer_id.clone(),
            options.caller_peer_id,
            options.request_timeout,
            options.router_port,
        )
        .await?;

        let worker = Self::from_transport(
            transport,
            Some(target_peer_id.clone()),
            Some(service_process),
        );
        wait_for_service_peer(
            &worker.inner.transport,
            &target_peer_id,
            options.request_timeout,
        )
        .await?;
        Ok(worker)
    }

    fn from_transport(
        transport: CallerTransport,
        default_target_peer_id: Option<String>,
        service_process: Option<Child>,
    ) -> Self {
        let runtime = ParentRuntime {
            transport,
            default_target_peer_id,
            metrics: Some(Metrics::new()),
            service_process: Arc::new(Mutex::new(service_process)),
        };
        Self {
            inner: Arc::new(runtime),
        }
    }

    pub fn handle(self) -> Handle {
        Handle { inner: self.inner }
    }

    pub fn peer_id(&self) -> &str {
        self.inner.transport.peer_id()
    }

    pub fn default_target_peer_id(&self) -> Option<&str> {
        self.inner.default_target_peer_id.as_deref()
    }

    pub async fn start(&self) -> Result<()> {
        Ok(())
    }

    pub async fn stop(&self) {
        let _ = self.inner.transport.disconnect().await;
        if let Some(mut child) = self.inner.service_process.lock().await.take() {
            let _ = child.kill();
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

impl Handle {
    pub async fn start(&self) -> Result<()> {
        Ok(())
    }

    pub async fn stop(&self) {
        let _ = self.inner.transport.disconnect().await;
        if let Some(mut child) = self.inner.service_process.lock().await.take() {
            let _ = child.kill();
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
pub struct ParentWorkerBuilder {
    mode: BuilderMode,
    target_peer_id: String,
    executable: Option<String>,
    options: BuilderOptions,
}

#[derive(Clone)]
enum BuilderMode {
    Spawn,
    Connect,
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

impl ParentWorkerBuilder {
    pub fn spawn(service_entrypoint: &str, executable: &str) -> Self {
        Self {
            mode: BuilderMode::Spawn,
            target_peer_id: canonical_peer_path(PathBuf::from(service_entrypoint)),
            executable: Some(executable.to_string()),
            options: BuilderOptions::default(),
        }
    }

    pub fn connect(target_peer_id: &str) -> Self {
        Self {
            mode: BuilderMode::Connect,
            target_peer_id: target_peer_id.to_string(),
            executable: None,
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

    pub async fn build(self) -> Result<ParentWorker> {
        match self.mode {
            BuilderMode::Spawn => {
                let executable = self.executable.ok_or_else(|| {
                    MultifrostError::ConfigError("spawn builder missing executable".into())
                })?;
                ParentWorker::spawn_with_options(
                    &self.target_peer_id,
                    &executable,
                    SpawnOptions {
                        caller_peer_id: self.options.caller_peer_id,
                        request_timeout: self.options.request_timeout,
                        router_port: self.options.router_port,
                    },
                )
                .await
            }
            BuilderMode::Connect => {
                ParentWorker::connect_with_options(
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

async fn wait_for_service_peer(
    transport: &CallerTransport,
    target_peer_id: &str,
    timeout: Option<Duration>,
) -> Result<()> {
    let timeout = timeout.unwrap_or(DEFAULT_REQUEST_TIMEOUT);
    let start = tokio::time::Instant::now();
    while start.elapsed() < timeout {
        match transport.query_peer_exists(target_peer_id).await {
            Ok(response) if response.exists => return Ok(()),
            _ => tokio::time::sleep(Duration::from_millis(100)).await,
        }
    }
    Err(MultifrostError::PeerNotFound(target_peer_id.to_string()))
}

fn spawn_service_process(
    executable: &str,
    service_entrypoint: &str,
    router_port: Option<u16>,
) -> Result<Child> {
    let mut command = if executable.split_whitespace().count() > 1 {
        if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.arg("/C").arg(executable);
            command
        } else {
            let mut command = Command::new("sh");
            command.arg("-c").arg(executable);
            command
        }
    } else {
        Command::new(executable)
    };
    command.env(
        "MULTIFROST_ENTRYPOINT_PATH",
        canonical_peer_path(PathBuf::from(service_entrypoint)),
    );
    if let Some(port) = router_port {
        command.env("MULTIFROST_ROUTER_PORT", port.to_string());
    }

    command.stdout(Stdio::null());
    command.stderr(Stdio::null());

    command.spawn().map_err(Into::into)
}

fn canonical_peer_path(path: PathBuf) -> String {
    path.canonicalize()
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn default_timeout_is_present() {
        let options = SpawnOptions::default();
        assert!(options.request_timeout.is_some());
    }

    #[test]
    fn builder_connect_uses_target_peer_id() {
        let builder = ParentWorkerBuilder::connect("math-service");
        assert_eq!(builder.target_peer_id, "math-service");
    }

    #[test]
    fn connect_options_default_to_timeout() {
        let options = ConnectOptions::default();
        assert!(options.request_timeout.is_some());
        assert!(options.router_port.is_none());
    }

    #[test]
    fn canonical_peer_path_uses_existing_absolute_path() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("service.rs");
        fs::write(&file, "fn main() {}").unwrap();

        let canonical = canonical_peer_path(file.clone());
        assert_eq!(canonical, file.canonicalize().unwrap().to_string_lossy());
    }

    #[test]
    fn spawn_service_process_exports_entrypoint_and_router_port() {
        let dir = tempdir().unwrap();
        let entrypoint = dir.path().join("worker.rs");
        fs::write(&entrypoint, "fn main() {}").unwrap();
        let output = dir.path().join("env.txt");
        let script = format!(
            "printf '%s|%s' \"$MULTIFROST_ENTRYPOINT_PATH\" \"$MULTIFROST_ROUTER_PORT\" > '{}'",
            output.display()
        );

        let mut child = spawn_service_process(&script, entrypoint.to_str().unwrap(), Some(4242))
            .expect("spawn service process");
        let status = child.wait().expect("wait for child");
        assert!(status.success());

        let contents = fs::read_to_string(&output).expect("read env output");
        let expected_entrypoint = entrypoint
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(contents, format!("{expected_entrypoint}|4242"));
    }
}
