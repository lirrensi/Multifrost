//! FILE: rust/src/process.rs
//! PURPOSE: Launch and manage spawned service processes for the v5 API.
//! OWNS: SpawnOptions, ServiceProcess, spawn, spawn_with_options.
//! EXPORTS: SpawnOptions, ServiceProcess, spawn, spawn_with_options.
//! DOCS: agent_chat/rust_v5_api_surface_2026-03-24.md

use crate::error::Result;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Duration;

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

#[derive(Debug)]
pub struct ServiceProcess {
    child: Option<Child>,
}

impl ServiceProcess {
    fn new(child: Child) -> Self {
        Self { child: Some(child) }
    }

    pub fn id(&self) -> Option<u32> {
        self.child.as_ref().map(|child| child.id())
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        Ok(())
    }

    pub fn wait(&mut self) -> std::io::Result<ExitStatus> {
        match self.child.as_mut() {
            Some(child) => child.wait(),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "service process already stopped",
            )),
        }
    }
}

pub async fn spawn(service_entrypoint: &str, executable: &str) -> Result<ServiceProcess> {
    spawn_with_options(service_entrypoint, executable, SpawnOptions::default()).await
}

pub async fn spawn_with_options(
    service_entrypoint: &str,
    executable: &str,
    options: SpawnOptions,
) -> Result<ServiceProcess> {
    let child = spawn_service_process(executable, service_entrypoint, options.router_port)?;
    Ok(ServiceProcess::new(child))
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

    #[tokio::test]
    async fn spawn_with_options_exports_entrypoint_and_router_port() {
        let dir = tempdir().unwrap();
        let entrypoint = dir.path().join("worker.rs");
        fs::write(&entrypoint, "fn main() {}").unwrap();
        let output = dir.path().join("env.txt");
        let script = format!(
            "printf '%s|%s' \"$MULTIFROST_ENTRYPOINT_PATH\" \"$MULTIFROST_ROUTER_PORT\" > '{}'",
            output.display()
        );

        let mut service_process = spawn_with_options(
            &entrypoint.to_string_lossy(),
            &script,
            SpawnOptions {
                caller_peer_id: None,
                request_timeout: None,
                router_port: Some(4242),
            },
        )
        .await
        .expect("spawn service process");
        let status = service_process.wait().expect("wait for child");
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
