use std::env;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use crate::error::{Result, RouterError};
use crate::protocol::{validate_protocol_key, PROTOCOL_KEY};

pub const DEFAULT_PORT: u16 = 9981;
pub const PORT_ENV_VAR: &str = "MULTIFROST_ROUTER_PORT";

#[derive(Debug, Clone)]
pub struct RouterConfig {
    protocol_key: &'static str,
    port: u16,
    bind_addr: SocketAddr,
    home_dir: PathBuf,
    runtime_dir: PathBuf,
    log_path: PathBuf,
    lock_path: PathBuf,
}

impl RouterConfig {
    pub fn load() -> Result<Self> {
        let port = parse_port(env::var(PORT_ENV_VAR).ok())?;
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let runtime_dir = home_dir.join(".multifrost");
        let log_path = runtime_dir.join("router.log");
        let lock_path = runtime_dir.join("router.lock");

        validate_protocol_key(PROTOCOL_KEY)?;

        Ok(Self {
            protocol_key: PROTOCOL_KEY,
            port,
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            home_dir,
            runtime_dir,
            log_path,
            lock_path,
        })
    }

    pub fn protocol_key(&self) -> &'static str {
        self.protocol_key
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    pub fn home_dir(&self) -> &Path {
        &self.home_dir
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    pub fn ensure_runtime_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.runtime_dir)?;
        Ok(())
    }
}

fn parse_port(value: Option<String>) -> Result<u16> {
    match value {
        Some(raw) => {
            let port = raw
                .parse::<u16>()
                .map_err(|_| RouterError::InvalidPort(raw.clone()))?;
            if port == 0 {
                return Err(RouterError::InvalidPort(raw));
            }
            Ok(port)
        }
        None => Ok(DEFAULT_PORT),
    }
}
