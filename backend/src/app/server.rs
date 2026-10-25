use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{Json, Router, routing::get};
use bollard::Docker;
use tokio::{
    sync::{Semaphore, watch},
    time::timeout,
};

use super::{
    config::{sandbox, server, terminal::MAX_SESSIONS},
    health,
    sandbox::cleanup_sandboxes,
    state::App,
    terminal::terminal,
};
use crate::lessons;

pub async fn run() -> Result<()> {
    let addr = server::listen_addr()?;
    if std::env::args().any(|arg| arg == "--healthcheck") {
        return health::check(addr).await;
    }
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("Cannot bind {addr}"))?;
    let docker = Docker::connect_with_local_defaults()?
        .negotiate_version()
        .await?;
    docker.ping().await.context("Docker недоступен")?;
    let image = sandbox::image();
    docker
        .inspect_image(&image)
        .await
        .context("Сначала собери учебный образ: docker compose build sandbox")?;
    cleanup_sandboxes(&docker).await?;
    let (shutdown_tx, shutdown) = watch::channel(false);
    let app = App {
        docker: docker.clone(),
        image,
        origins: server::origins(),
        slots: Arc::new(Semaphore::new(MAX_SESSIONS)),
        shutdown,
    };
    let sessions = app.slots.clone();
    let router = Router::new()
        .route("/api/health", get(health::status))
        .route("/api/course", get(|| async { Json(lessons::course()) }))
        .route("/api/lessons", get(|| async { Json(lessons::all()) }))
        .route("/api/terminal/{lesson}", get(terminal))
        .with_state(app);
    tracing::info!(%addr, "LinuxCatch запущен");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            let _ = shutdown_tx.send(true);
        })
        .await?;
    if timeout(
        server::SHUTDOWN_TIMEOUT,
        sessions.acquire_many(MAX_SESSIONS as u32),
    )
    .await
    .is_err()
    {
        tracing::warn!("Ожидание завершения учебных сессий превысило лимит");
    }
    cleanup_sandboxes(&docker).await?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("Cannot register SIGTERM");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
