use crate::error::{MultifrostError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process;
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceRegistration {
    port: u16,
    pid: u32,
    started: String,
}

type Registry = HashMap<String, ServiceRegistration>;

pub struct ServiceRegistry;

impl ServiceRegistry {
    fn registry_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".multifrost")
            .join("services.json")
    }

    async fn ensure_registry_dir() -> Result<()> {
        let dir = Self::registry_path()
            .parent()
            .ok_or_else(|| MultifrostError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Invalid registry path"
            )))?
            .to_path_buf();
        
        fs::create_dir_all(&dir).await?;
        Ok(())
    }

    async fn read_registry() -> Result<Registry> {
        Self::ensure_registry_dir().await?;
        let path = Self::registry_path();
        
        match fs::read_to_string(&path).await {
            Ok(data) => Ok(serde_json::from_str(&data)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(MultifrostError::IoError(e)),
        }
    }

    async fn write_registry(registry: &Registry) -> Result<()> {
        Self::ensure_registry_dir().await?;
        let path = Self::registry_path();
        let temp_path = path.with_extension("json.tmp");
        
        let data = serde_json::to_string_pretty(registry)?;
        
        let mut file = fs::File::create(&temp_path).await?;
        file.write_all(data.as_bytes()).await?;
        file.sync_all().await?;
        drop(file);
        
        fs::rename(&temp_path, &path).await?;
        Ok(())
    }

    fn is_process_alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            use std::process::Command;
            Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
        #[cfg(windows)]
        {
            use std::process::Command;
            Command::new("tasklist")
                .args(["/FI", &format!("PID eq {}", pid)])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
                .unwrap_or(false)
        }
    }

    pub async fn register(service_id: &str) -> Result<u16> {
        let mut registry = Self::read_registry().await?;
        
        // Check if service already running
        if let Some(existing) = registry.get(service_id) {
            if Self::is_process_alive(existing.pid) {
                return Err(MultifrostError::ServiceAlreadyRunning(service_id.to_string()));
            }
            // Dead process - will be overwritten
        }
        
        let port = Self::find_free_port().await?;
        
        registry.insert(service_id.to_string(), ServiceRegistration {
            port,
            pid: process::id(),
            started: chrono_lite_timestamp(),
        });
        
        Self::write_registry(&registry).await?;
        Ok(port)
    }

    pub async fn discover(service_id: &str, timeout_ms: u64) -> Result<u16> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
        
        loop {
            let registry = Self::read_registry().await?;
            
            if let Some(reg) = registry.get(service_id) {
                if Self::is_process_alive(reg.pid) {
                    return Ok(reg.port);
                }
            }
            
            if tokio::time::Instant::now() >= deadline {
                return Err(MultifrostError::ServiceNotFound(service_id.to_string()));
            }
            
            sleep(Duration::from_millis(100)).await;
        }
    }

    pub async fn unregister(service_id: &str) -> Result<()> {
        let mut registry = Self::read_registry().await?;
        let current_pid = process::id();
        
        if let Some(reg) = registry.get(service_id) {
            if reg.pid == current_pid {
                registry.remove(service_id);
                Self::write_registry(&registry).await?;
            }
        }
        
        Ok(())
    }

    pub async fn find_free_port() -> Result<u16> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        drop(listener);
        Ok(port)
    }
}

fn chrono_lite_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}
