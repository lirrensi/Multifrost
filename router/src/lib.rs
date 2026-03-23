pub mod config;
pub mod error;
pub mod protocol;
pub mod registry;
pub mod server;

use std::fs::OpenOptions;
use std::io::Write;

use tokio::net::TcpListener;

use crate::config::RouterConfig;
use crate::error::Result;
use crate::registry::PeerRegistry;

pub async fn run() -> Result<()> {
    let config = RouterConfig::load()?;
    config.ensure_runtime_dir()?;

    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(config.log_path())?;
    writeln!(log, "multifrost-router starting on {}", config.bind_addr())?;

    let listener = TcpListener::bind(config.bind_addr()).await?;
    let registry = PeerRegistry::new();
    server::serve(listener, registry).await
}
