use crate::error::{MultifrostError, Result};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tokio::time::{sleep, Instant};
use tokio_tungstenite::connect_async;

pub const DEFAULT_ROUTER_PORT: u16 = 9981;
pub const ROUTER_PORT_ENV: &str = "MULTIFROST_ROUTER_PORT";
pub const ROUTER_LOCK_PATH: &str = ".multifrost/router.lock";
pub const ROUTER_LOG_PATH: &str = ".multifrost/router.log";
pub const ROUTER_BIN_ENV: &str = "MULTIFROST_ROUTER_BIN";

#[derive(Debug, Clone)]
pub struct RouterEndpointConfig {
    pub host: String,
    pub port: u16,
}

impl RouterEndpointConfig {
    pub fn from_env() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: router_port_from_env(),
        }
    }

    pub fn ws_url(&self) -> String {
        format!("ws://{}:{}", self.host, self.port)
    }
}

#[derive(Debug, Clone)]
pub struct RouterBootstrapConfig {
    pub endpoint: RouterEndpointConfig,
    pub lock_path: PathBuf,
    pub log_path: PathBuf,
    pub readiness_timeout: Duration,
    pub readiness_poll_interval: Duration,
}

impl RouterBootstrapConfig {
    pub fn from_env() -> Self {
        Self {
            endpoint: RouterEndpointConfig::from_env(),
            lock_path: default_router_lock_path(),
            log_path: default_router_log_path(),
            readiness_timeout: Duration::from_secs(10),
            readiness_poll_interval: Duration::from_millis(100),
        }
    }
}

pub fn router_port_from_env() -> u16 {
    std::env::var(ROUTER_PORT_ENV)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(DEFAULT_ROUTER_PORT)
}

pub fn default_router_lock_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(ROUTER_LOCK_PATH)
}

pub fn default_router_log_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(ROUTER_LOG_PATH)
}

pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir().or_else(|| std::env::var_os("HOME").map(PathBuf::from))
}

pub fn router_ws_url(port: u16) -> String {
    format!("ws://127.0.0.1:{port}")
}

pub async fn router_is_reachable(endpoint: &RouterEndpointConfig) -> bool {
    connect_async(endpoint.ws_url()).await.is_ok()
}

pub async fn ensure_router(endpoint: &RouterEndpointConfig) -> Result<()> {
    if router_is_reachable(endpoint).await {
        Ok(())
    } else {
        Err(MultifrostError::RouterUnavailable(endpoint.ws_url()))
    }
}

pub async fn bootstrap_router(config: &RouterBootstrapConfig) -> Result<()> {
    if router_is_reachable(&config.endpoint).await {
        return Ok(());
    }

    let _lock = acquire_lock(&config.lock_path, Duration::from_secs(10)).await?;

    if router_is_reachable(&config.endpoint).await {
        return Ok(());
    }

    let mut child = spawn_router_process(config)?;
    let deadline = Instant::now() + config.readiness_timeout;

    loop {
        if router_is_reachable(&config.endpoint).await {
            let _ = child.id();
            return Ok(());
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err(MultifrostError::BootstrapFailure(format!(
                "router did not become reachable at {} within {:?}",
                config.endpoint.ws_url(),
                config.readiness_timeout
            )));
        }

        sleep(config.readiness_poll_interval).await;
    }
}

pub async fn connect_or_bootstrap(config: &RouterBootstrapConfig) -> Result<()> {
    if router_is_reachable(&config.endpoint).await {
        return Ok(());
    }
    bootstrap_router(config).await
}

async fn acquire_lock(lock_path: &Path, stale_after: Duration) -> Result<RouterLockGuard> {
    if let Some(parent) = lock_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(lock_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                let _ = write!(file, "{}", chrono_like_now());
                return Ok(RouterLockGuard {
                    path: lock_path.to_path_buf(),
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                if is_lock_stale(lock_path, stale_after).await {
                    let _ = tokio::fs::remove_file(lock_path).await;
                    continue;
                }
                if Instant::now() >= deadline {
                    return Err(MultifrostError::BootstrapFailure(
                        "timed out waiting for router bootstrap lock".into(),
                    ));
                }
                sleep(Duration::from_millis(100)).await;
            }
            Err(err) => return Err(err.into()),
        }
    }
}

async fn is_lock_stale(lock_path: &Path, stale_after: Duration) -> bool {
    tokio::fs::metadata(lock_path)
        .await
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .map(|elapsed| elapsed > stale_after)
        .unwrap_or(false)
}

fn spawn_router_process(config: &RouterBootstrapConfig) -> Result<Child> {
    let router_bin = std::env::var(ROUTER_BIN_ENV).unwrap_or_else(|_| "multifrost-router".into());
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.log_path)?;

    let mut command = Command::new(router_bin);
    command.env(ROUTER_PORT_ENV, config.endpoint.port.to_string());
    command.stdout(Stdio::from(log_file.try_clone()?));
    command.stderr(Stdio::from(log_file));

    command
        .spawn()
        .map_err(|err| MultifrostError::BootstrapFailure(err.to_string()))
}

fn chrono_like_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

struct RouterLockGuard {
    path: PathBuf,
}

impl Drop for RouterLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    #[test]
    fn router_endpoint_formats_ws_url() {
        let endpoint = RouterEndpointConfig {
            host: "127.0.0.1".into(),
            port: 9981,
        };
        assert_eq!(endpoint.ws_url(), "ws://127.0.0.1:9981");
    }

    #[test]
    #[serial]
    fn router_port_from_env_accepts_valid_override() {
        let original = std::env::var(ROUTER_PORT_ENV).ok();
        std::env::set_var(ROUTER_PORT_ENV, "12345");
        assert_eq!(router_port_from_env(), 12345);
        match original {
            Some(value) => std::env::set_var(ROUTER_PORT_ENV, value),
            None => std::env::remove_var(ROUTER_PORT_ENV),
        }
    }

    #[test]
    #[serial]
    fn router_port_from_env_falls_back_for_invalid_values() {
        let original = std::env::var(ROUTER_PORT_ENV).ok();
        std::env::set_var(ROUTER_PORT_ENV, "not-a-port");
        assert_eq!(router_port_from_env(), DEFAULT_ROUTER_PORT);
        match original {
            Some(value) => std::env::set_var(ROUTER_PORT_ENV, value),
            None => std::env::remove_var(ROUTER_PORT_ENV),
        }
    }

    async fn start_ws_server() -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ws = accept_async(stream).await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        (format!("ws://{}", addr), handle)
    }

    #[tokio::test]
    async fn router_is_reachable_returns_true_for_websocket_server() {
        let (url, handle) = start_ws_server().await;
        let endpoint = RouterEndpointConfig {
            host: "127.0.0.1".into(),
            port: url
                .rsplit_once(':')
                .and_then(|(_, port)| port.parse::<u16>().ok())
                .unwrap(),
        };
        assert!(router_is_reachable(&endpoint).await);
        handle.abort();
    }

    #[tokio::test]
    async fn ensure_router_reports_unreachable_server() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = listener.accept().await;
        });
        let endpoint = RouterEndpointConfig {
            host: "127.0.0.1".into(),
            port: addr.port(),
        };
        let err = ensure_router(&endpoint).await.unwrap_err();
        assert!(matches!(err, MultifrostError::RouterUnavailable(_)));
        handle.abort();
    }
}
