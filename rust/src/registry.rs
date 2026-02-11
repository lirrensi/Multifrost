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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_find_free_port() {
        let port1 = ServiceRegistry::find_free_port().await.unwrap();
        let port2 = ServiceRegistry::find_free_port().await.unwrap();

        // Ports should be different (most likely)
        // and in valid range
        assert!(port1 >= 1024 && port1 <= 65535);
        assert!(port2 >= 1024 && port2 <= 65535);
    }

    #[tokio::test]
    async fn test_register_service() {
        let service_id = "test-service-1";
        let port = ServiceRegistry::register(service_id).await.unwrap();

        assert!(port >= 1024 && port <= 65535);

        // Clean up
        let _ = ServiceRegistry::unregister(service_id).await;
    }

    #[tokio::test]
    async fn test_register_duplicate_service() {
        let service_id = "test-service-duplicate";

        // First registration should succeed
        let port1 = ServiceRegistry::register(service_id).await.unwrap();
        assert!(port1 >= 1024);

        // Second registration should fail
        let result = ServiceRegistry::register(service_id).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(MultifrostError::ServiceAlreadyRunning(_))));

        // Clean up
        let _ = ServiceRegistry::unregister(service_id).await;
    }

    #[tokio::test]
    async fn test_discover_service() {
        let service_id = "test-service-discover";

        // Register service
        let port = ServiceRegistry::register(service_id).await.unwrap();

        // Discover service
        let discovered_port = ServiceRegistry::discover(service_id, 1000).await.unwrap();
        assert_eq!(discovered_port, port);

        // Clean up
        let _ = ServiceRegistry::unregister(service_id).await;
    }

    #[tokio::test]
    async fn test_discover_nonexistent_service() {
        let service_id = "test-service-nonexistent";

        let result = ServiceRegistry::discover(service_id, 100).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(MultifrostError::ServiceNotFound(_))));
    }

    #[tokio::test]
    async fn test_unregister_service() {
        let service_id = "test-service-unregister";

        // Register service
        ServiceRegistry::register(service_id).await.unwrap();

        // Unregister service
        let result = ServiceRegistry::unregister(service_id).await;
        assert!(result.is_ok());

        // Service should no longer be discoverable
        let result = ServiceRegistry::discover(service_id, 100).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unregister_nonexistent_service() {
        let service_id = "test-service-nonexistent-unregister";

        // Unregistering a non-existent service should not fail
        let result = ServiceRegistry::unregister(service_id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_registry_persistence() {
        let service_id = "test-service-persistence";

        // Register service
        let _port1 = ServiceRegistry::register(service_id).await.unwrap();

        // Read registry directly
        let registry = ServiceRegistry::read_registry().await.unwrap();
        assert!(registry.contains_key(service_id));

        // Clean up
        let _ = ServiceRegistry::unregister(service_id).await;
    }

    #[tokio::test]
    async fn test_multiple_services() {
        let services = vec![
            "test-service-multi-1",
            "test-service-multi-2",
            "test-service-multi-3",
        ];

        let mut ports = Vec::new();
        for service_id in &services {
            let port = ServiceRegistry::register(service_id).await.unwrap();
            ports.push(port);
        }

        // All services should be discoverable
        for (i, service_id) in services.iter().enumerate() {
            let discovered_port = ServiceRegistry::discover(service_id, 1000).await.unwrap();
            assert_eq!(discovered_port, ports[i]);
        }

        // Clean up
        for service_id in &services {
            let _ = ServiceRegistry::unregister(service_id).await;
        }
    }

    #[tokio::test]
    async fn test_service_registration_fields() {
        let service_id = "test-service-fields";

        let port = ServiceRegistry::register(service_id).await.unwrap();

        // Read registry and check fields
        let registry = ServiceRegistry::read_registry().await.unwrap();
        let registration = registry.get(service_id).unwrap();

        assert_eq!(registration.port, port);
        assert_eq!(registration.pid, std::process::id());
        assert!(!registration.started.is_empty());

        // Clean up
        let _ = ServiceRegistry::unregister(service_id).await;
    }

    #[tokio::test]
    async fn test_discover_timeout() {
        let service_id = "test-service-timeout";

        // Try to discover a non-existent service with short timeout
        let start = std::time::Instant::now();
        let result = ServiceRegistry::discover(service_id, 100).await;
        let elapsed = start.elapsed();

        assert!(result.is_err());
        // Should timeout after approximately 100ms
        assert!(elapsed.as_millis() >= 90);
        assert!(elapsed.as_millis() < 200);
    }

    #[tokio::test]
    async fn test_concurrent_registrations() {
        let mut handles = vec![];

        for i in 0..10 {
            let handle = tokio::spawn(async move {
                let service_id = format!("test-service-concurrent-{}", i);
                let result = ServiceRegistry::register(&service_id).await;
                (service_id, result)
            });
            handles.push(handle);
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // All registrations should succeed
        for (service_id, result) in results {
            assert!(result.is_ok(), "Failed to register {}", service_id);
            let _ = ServiceRegistry::unregister(&service_id).await;
        }
    }

    #[tokio::test]
    async fn test_registry_file_creation() {
        let service_id = "test-service-file-creation";

        // Ensure registry doesn't exist
        let _ = ServiceRegistry::unregister(service_id).await;

        // Register a service (should create registry file)
        let _ = ServiceRegistry::register(service_id).await.unwrap();

        // Check that registry file exists
        let registry_path = ServiceRegistry::registry_path();
        assert!(tokio::fs::metadata(&registry_path).await.is_ok());

        // Clean up
        let _ = ServiceRegistry::unregister(service_id).await;
    }

    #[test]
    fn test_is_process_alive_current_process() {
        let current_pid = std::process::id();
        assert!(ServiceRegistry::is_process_alive(current_pid));
    }

    #[test]
    fn test_is_process_alive_nonexistent_process() {
        // Use a PID that's unlikely to exist
        let nonexistent_pid = 999999;
        assert!(!ServiceRegistry::is_process_alive(nonexistent_pid));
    }

    #[tokio::test]
    async fn test_overwrite_dead_process_registration() {
        let service_id = "test-service-dead-process";

        // Register service
        let port1 = ServiceRegistry::register(service_id).await.unwrap();

        // Manually modify registry to have a dead PID
        let mut registry = ServiceRegistry::read_registry().await.unwrap();
        if let Some(ref mut reg) = registry.get_mut(service_id) {
            reg.pid = 999999; // Non-existent PID
        }
        ServiceRegistry::write_registry(&registry).await.unwrap();

        // Should be able to register again (dead process will be overwritten)
        let port2 = ServiceRegistry::register(service_id).await.unwrap();
        assert!(port2 >= 1024);

        // Clean up
        let _ = ServiceRegistry::unregister(service_id).await;
    }
}
