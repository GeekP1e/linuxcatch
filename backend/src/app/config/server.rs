use std::{net::SocketAddr, time::Duration};

use anyhow::{Context, Result};

pub(crate) const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
pub(crate) const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn listen_addr() -> Result<SocketAddr> {
    std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".into())
        .parse()
        .context("LISTEN_ADDR должен содержать IP-адрес и порт")
}

pub(crate) fn origins() -> Vec<String> {
    std::env::var("APP_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:8080,http://127.0.0.1:8080,http://localhost:5173,http://127.0.0.1:5173".into())
        .split(',')
        .map(|origin| origin.trim().to_owned())
        .collect()
}
