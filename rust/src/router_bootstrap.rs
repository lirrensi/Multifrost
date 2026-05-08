//! FILE: rust/src/router_bootstrap.rs
//! PURPOSE: Implement router bootstrap with JSON lock file for dead-holder detection and skip-spawn optimization.
//! OWNS: Router reachability, bootstrap lock acquisition and evaluation, router process spawning.
//! EXPORTS: RouterEndpointConfig, RouterBootstrapConfig, router_port_from_env, default_router_lock_path,
//!          default_router_log_path, router_ws_url, router_is_reachable, ensure_router, bootstrap_router,
//!          connect_or_bootstrap.
//! DOCS: docs/spec.md (Lock File Format section), agent_chat/plan_json_lock_file_2026-05-08.md

use crate::error::{MultifrostError, Result};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
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

    let lock = match acquire_lock(
        &config.lock_path,
        config.endpoint.port,
        Duration::from_secs(10),
    )
    .await?
    {
        Some(lock) => lock,
        None => {
            // SkipSpawn: another process has a running router, wait for it
            let deadline = Instant::now() + config.readiness_timeout;
            while Instant::now() < deadline {
                if router_is_reachable(&config.endpoint).await {
                    return Ok(());
                }
                sleep(config.readiness_poll_interval).await;
            }
            return Err(MultifrostError::BootstrapFailure(
                "timeout waiting for router to become reachable (skip_spawn)".into(),
            ));
        }
    };

    // We hold the lock. Double-check reachability before spawning.
    if router_is_reachable(&config.endpoint).await {
        lock.update(|d| d.status = "ready".to_string());
        return Ok(());
    }

    let mut child = spawn_router_process(config)?;
    let router_pid = child.id();
    lock.update(|d| d.router_pid = Some(router_pid));

    let deadline = Instant::now() + config.readiness_timeout;
    loop {
        if router_is_reachable(&config.endpoint).await {
            lock.update(|d| d.status = "ready".to_string());
            return Ok(());
        }

        if Instant::now() >= deadline {
            lock.update(|d| d.status = "failed".to_string());
            let _ = child.kill();
            return Err(MultifrostError::BootstrapFailure(format!(
                "router did not become reachable at {} within {:?}",
                config.endpoint.ws_url(),
                config.readiness_timeout
            )));
        }

        sleep(config.readiness_poll_interval).await;
    }
    // AcquiredLock Drop impl removes the lock file on scope exit.
}

pub async fn connect_or_bootstrap(config: &RouterBootstrapConfig) -> Result<()> {
    if router_is_reachable(&config.endpoint).await {
        return Ok(());
    }
    bootstrap_router(config).await
}

// ---------------------------------------------------------------------------
// Lock file types
// ---------------------------------------------------------------------------

/// On-disk JSON lock file contents.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockFileData {
    format: String,
    pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    router_pid: Option<u32>,
    port: u16,
    created_at_unix: f64,
    expires_at_unix: f64,
    status: String,
    #[serde(default)]
    language: String,
}

/// Result of evaluating an existing lock file.
#[derive(Debug, Clone, PartialEq)]
enum LockEvalResult {
    /// Lock is stale / invalid — can be reclaimed.
    Reclaim,
    /// Lock holder has a running router — skip spawning, wait for reachability.
    SkipSpawn,
    /// Lock is still valid — retry later.
    Wait,
}

/// Guard representing an acquired bootstrap lock. The lock file is removed on drop.
struct AcquiredLock {
    path: PathBuf,
    data: std::sync::Mutex<LockFileData>,
}

impl AcquiredLock {
    /// Apply a mutation to the in-memory data and persist it to the lock file.
    fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut LockFileData),
    {
        let mut data = self.data.lock().unwrap();
        f(&mut data);
        let json = serde_json::to_string(&*data).unwrap();
        let _ = std::fs::write(&self.path, json);
    }
}

impl Drop for AcquiredLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

// ---------------------------------------------------------------------------
// Process liveness check
// ---------------------------------------------------------------------------

fn is_process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    platform_is_process_alive(pid)
}

#[cfg(unix)]
fn platform_is_process_alive(pid: u32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn platform_is_process_alive(pid: u32) -> bool {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/FO", "csv", "/NH"])
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains(&format!("\"{}\"", pid)),
        Err(_) => false,
    }
}

#[cfg(not(any(unix, windows)))]
fn platform_is_process_alive(_pid: u32) -> bool {
    false
}

// ---------------------------------------------------------------------------
// Lock file evaluation (6 rules from spec)
// ---------------------------------------------------------------------------

fn evaluate_existing_lock(path: &Path) -> LockEvalResult {
    // Rule 1: Can't read or can't parse → Reclaim
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return LockEvalResult::Reclaim,
    };
    let lock: LockFileData = match serde_json::from_str(&content) {
        Ok(l) => l,
        Err(_) => return LockEvalResult::Reclaim,
    };
    // Also check that format is "v1" (also part of rule 1)
    if lock.format != "v1" {
        return LockEvalResult::Reclaim;
    }

    // Rule 2: expires_at_unix < now → Reclaim
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    if lock.expires_at_unix < now {
        return LockEvalResult::Reclaim;
    }

    // Rule 3: pid is not alive → Reclaim
    if !is_process_alive(lock.pid) {
        return LockEvalResult::Reclaim;
    }

    // Rule 4: status == "failed" → Reclaim
    if lock.status == "failed" {
        return LockEvalResult::Reclaim;
    }

    // Rule 5: router_pid is set and alive → SkipSpawn
    if let Some(router_pid) = lock.router_pid {
        if is_process_alive(router_pid) {
            return LockEvalResult::SkipSpawn;
        }
    }

    // Rule 6: Otherwise → Wait
    LockEvalResult::Wait
}

// ---------------------------------------------------------------------------
// Lock acquisition
// ---------------------------------------------------------------------------

async fn acquire_lock(
    lock_path: &Path,
    port: u16,
    stale_after: Duration,
) -> Result<Option<AcquiredLock>> {
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
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64();
                let data = LockFileData {
                    format: "v1".to_string(),
                    pid: std::process::id(),
                    router_pid: None,
                    port,
                    created_at_unix: now,
                    expires_at_unix: now + stale_after.as_secs_f64(),
                    status: "starting".to_string(),
                    language: "rust".to_string(),
                };
                let json = serde_json::to_string(&data)?;
                file.write_all(json.as_bytes())?;
                return Ok(Some(AcquiredLock {
                    path: lock_path.to_path_buf(),
                    data: std::sync::Mutex::new(data),
                }));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                match evaluate_existing_lock(lock_path) {
                    LockEvalResult::Reclaim => {
                        let _ = tokio::fs::remove_file(lock_path).await;
                        continue;
                    }
                    LockEvalResult::SkipSpawn => {
                        return Ok(None);
                    }
                    LockEvalResult::Wait => {
                        if Instant::now() >= deadline {
                            return Err(MultifrostError::BootstrapFailure(
                                "timed out waiting for router bootstrap lock".into(),
                            ));
                        }
                        sleep(Duration::from_millis(100)).await;
                    }
                }
            }
            Err(err) => return Err(err.into()),
        }
    }
}

// ---------------------------------------------------------------------------
// Router process spawning
// ---------------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // Lock evaluation tests
    // -----------------------------------------------------------------------

    #[test]
    fn is_process_alive_returns_true_for_current_pid() {
        assert!(is_process_alive(std::process::id()));
    }

    #[test]
    fn is_process_alive_returns_false_for_zero() {
        assert!(!is_process_alive(0));
    }

    #[test]
    fn is_process_alive_returns_false_for_bogus_pid() {
        // PID 1 is init/systemd on Unix, unlikely to be ours. On Windows,
        // use a very large PID that almost certainly doesn't exist.
        assert!(!is_process_alive(u32::MAX));
    }

    #[test]
    fn evaluate_existing_lock_reclaims_on_missing_file() {
        let path = PathBuf::from(std::env::temp_dir().join("__mf_test_nonexistent_lock.json"));
        let _ = std::fs::remove_file(&path); // clean up if left over
        assert_eq!(evaluate_existing_lock(&path), LockEvalResult::Reclaim);
    }

    #[test]
    fn evaluate_existing_lock_reclaims_on_invalid_json() {
        let dir = std::env::temp_dir();
        let path = dir.join("__mf_test_invalid_json.json");
        let _ = std::fs::write(&path, "not json");
        assert_eq!(evaluate_existing_lock(&path), LockEvalResult::Reclaim);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn evaluate_existing_lock_reclaims_on_wrong_format() {
        let dir = std::env::temp_dir();
        let path = dir.join("__mf_test_wrong_format.json");
        let data = LockFileData {
            format: "v0".into(),
            pid: std::process::id(),
            router_pid: None,
            port: 9981,
            created_at_unix: 0.0,
            expires_at_unix: f64::MAX,
            status: "starting".into(),
            language: "rust".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let _ = std::fs::write(&path, &json);
        assert_eq!(evaluate_existing_lock(&path), LockEvalResult::Reclaim);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn evaluate_existing_lock_reclaims_on_expired() {
        let dir = std::env::temp_dir();
        let path = dir.join("__mf_test_expired.json");
        let data = LockFileData {
            format: "v1".into(),
            pid: std::process::id(),
            router_pid: None,
            port: 9981,
            created_at_unix: 1000.0,
            expires_at_unix: 1001.0, // well in the past
            status: "starting".into(),
            language: "rust".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let _ = std::fs::write(&path, &json);
        assert_eq!(evaluate_existing_lock(&path), LockEvalResult::Reclaim);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn evaluate_existing_lock_reclaims_on_dead_pid() {
        let dir = std::env::temp_dir();
        let path = dir.join("__mf_test_dead_pid.json");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let data = LockFileData {
            format: "v1".into(),
            pid: u32::MAX, // almost certainly dead
            router_pid: None,
            port: 9981,
            created_at_unix: now,
            expires_at_unix: now + 3600.0, // far in the future
            status: "starting".into(),
            language: "rust".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let _ = std::fs::write(&path, &json);
        assert_eq!(evaluate_existing_lock(&path), LockEvalResult::Reclaim);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn evaluate_existing_lock_reclaims_on_failed_status() {
        let dir = std::env::temp_dir();
        let path = dir.join("__mf_test_failed_status.json");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let data = LockFileData {
            format: "v1".into(),
            pid: std::process::id(),
            router_pid: None,
            port: 9981,
            created_at_unix: now,
            expires_at_unix: now + 3600.0,
            status: "failed".into(),
            language: "rust".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let _ = std::fs::write(&path, &json);
        assert_eq!(evaluate_existing_lock(&path), LockEvalResult::Reclaim);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn evaluate_existing_lock_waits_on_valid_starting() {
        let dir = std::env::temp_dir();
        let path = dir.join("__mf_test_valid_starting.json");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let data = LockFileData {
            format: "v1".into(),
            pid: std::process::id(),
            router_pid: None,
            port: 9981,
            created_at_unix: now,
            expires_at_unix: now + 3600.0,
            status: "starting".into(),
            language: "rust".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let _ = std::fs::write(&path, &json);
        assert_eq!(evaluate_existing_lock(&path), LockEvalResult::Wait);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn evaluate_existing_lock_skip_spawn_when_router_alive() {
        // We can't easily create a subprocess with a known PID for this test,
        // so we test the condition that router_pid is Some and alive equals
        // our own PID. Since our PID is alive, with router_pid set to it,
        // we should get SkipSpawn.
        let dir = std::env::temp_dir();
        let path = dir.join("__mf_test_skip_spawn.json");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let data = LockFileData {
            format: "v1".into(),
            pid: std::process::id(),
            router_pid: Some(std::process::id()), // our own PID, definitely alive
            port: 9981,
            created_at_unix: now,
            expires_at_unix: now + 3600.0,
            status: "starting".into(),
            language: "rust".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let _ = std::fs::write(&path, &json);
        assert_eq!(evaluate_existing_lock(&path), LockEvalResult::SkipSpawn);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn serialized_lock_file_is_valid_json() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let data = LockFileData {
            format: "v1".into(),
            pid: 12345,
            router_pid: None,
            port: 9981,
            created_at_unix: now,
            expires_at_unix: now + 10.0,
            status: "starting".into(),
            language: "rust".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        // Round-trip
        let parsed: LockFileData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.format, "v1");
        assert_eq!(parsed.pid, 12345);
        assert!(parsed.router_pid.is_none());
        assert_eq!(parsed.port, 9981);
        assert_eq!(parsed.status, "starting");
        assert_eq!(parsed.language, "rust");
    }

    #[test]
    fn serialized_lock_file_omits_router_pid_when_none() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let data = LockFileData {
            format: "v1".into(),
            pid: 12345,
            router_pid: None,
            port: 9981,
            created_at_unix: now,
            expires_at_unix: now + 10.0,
            status: "starting".into(),
            language: "rust".into(),
        };
        let json = serde_json::to_string(&data).unwrap();
        // When router_pid is None, the field should not appear in the JSON
        assert!(!json.contains("router_pid"));
    }

    #[test]
    fn serialized_lock_file_defaults_language_when_missing() {
        let json = r#"{
            \"format\": \"v1\",
            \"pid\": 12345,
            \"port\": 9981,
            \"created_at_unix\": 1000.0,
            \"expires_at_unix\": 2000.0,
            \"status\": \"starting\"
        }"#;
        // Remove escapes for the actual test string
        let json = json
            .replace("\\\"", "\"")
            .replace("    ", "")
            .replace('\n', "");
        let parsed: LockFileData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.language, "");
        assert!(parsed.router_pid.is_none());
    }

    #[test]
    fn lock_round_trip_through_acquired_lock_update() {
        use std::sync::Mutex;

        let dir = std::env::temp_dir();
        let path = dir.join("__mf_test_lock_roundtrip.json");
        let _ = std::fs::remove_file(&path);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        let data = LockFileData {
            format: "v1".into(),
            pid: std::process::id(),
            router_pid: None,
            port: 9981,
            created_at_unix: now,
            expires_at_unix: now + 10.0,
            status: "starting".into(),
            language: "rust".into(),
        };
        // Write initial JSON so update() can re-read it
        let json = serde_json::to_string(&data).unwrap();
        std::fs::write(&path, &json).unwrap();

        let lock = AcquiredLock {
            path: path.clone(),
            data: Mutex::new(data),
        };

        lock.update(|d| d.status = "ready".to_string());

        // Read back and verify
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: LockFileData = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.status, "ready");

        // Clean up
        let _ = std::fs::remove_file(&path);
    }
}
