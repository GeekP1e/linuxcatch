use std::net::SocketAddr;

use anyhow::{Result, bail};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    time::timeout,
};

use super::{config::server::HEALTH_TIMEOUT, state::App};

pub(super) async fn check(addr: SocketAddr) -> Result<()> {
    let mut health_addr = addr;
    if health_addr.ip().is_unspecified() {
        health_addr.set_ip(if health_addr.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    timeout(HEALTH_TIMEOUT, async {
        let mut stream = tokio::net::TcpStream::connect(health_addr).await?;
        stream
            .write_all(b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await?;
        let mut status = String::new();
        BufReader::new(stream).read_line(&mut status).await?;
        if status.split_whitespace().nth(1) != Some("200") {
            bail!("Backend is unhealthy");
        }
        Ok(())
    })
    .await?
}

pub(super) async fn status(State(app): State<App>) -> Response {
    match timeout(HEALTH_TIMEOUT, app.docker.ping()).await {
        Ok(Ok(_)) => Json(json!({"status": "ok"})).into_response(),
        _ => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "docker_unavailable"})),
        )
            .into_response(),
    }
}
