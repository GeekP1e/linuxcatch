use std::{sync::Arc, time::Duration};

use axum::{Router, extract::WebSocketUpgrade, routing::get};
use bollard::Docker;
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Semaphore, watch},
    time::timeout,
};
use uuid::Uuid;

use super::{
    config::terminal::{IO_TIMEOUT, MAX_SESSIONS},
    sandbox::{remove_sandbox, run_command},
    state::App,
    terminal::{run_session, terminal},
};
use crate::lessons;
use tokio::sync::Notify;
use tokio_tungstenite::{connect_async, tungstenite::Message as ClientMessage};

// These tests use their own HTTP listener and container, without global cleanup.
async fn cancel_slow_preparation(shutdown_server: bool) {
    let docker = Docker::connect_with_local_defaults()
        .unwrap()
        .negotiate_version()
        .await
        .unwrap();
    let name = format!("linuxcatch-test-session-{}", Uuid::new_v4());
    let (shutdown_tx, shutdown) = watch::channel(false);
    let app = App {
        docker: docker.clone(),
        image: "linuxcatch-sandbox:local".into(),
        origins: vec![],
        slots: Arc::new(Semaphore::new(MAX_SESSIONS)),
        shutdown,
    };
    let done = Arc::new(Notify::new());
    let slots = app.slots.clone();
    let mut lesson = lessons::find("mkdir").unwrap();
    lesson.setup = "touch /tmp/linuxcatch-test-preparing; sleep 30".into();
    let router = Router::new().route(
        "/terminal",
        get({
            let name = name.clone();
            let done = done.clone();
            move |ws: WebSocketUpgrade| {
                let app = app.clone();
                let name = name.clone();
                let lesson = lesson.clone();
                let done = done.clone();
                async move {
                    ws.on_upgrade(move |mut socket| async move {
                        let _permit = app.slots.clone().acquire_owned().await.unwrap();
                        let _ = run_session(&app, &name, &lesson, &mut socket).await;
                        remove_sandbox(&app.docker, &name).await.unwrap();
                        done.notify_one();
                    })
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let (mut socket, _) = connect_async(format!("ws://{address}/terminal"))
        .await
        .unwrap();
    let prepared = timeout(Duration::from_secs(10), async {
        loop {
            if matches!(
                run_command(
                    &docker,
                    &name,
                    "test -f /tmp/linuxcatch-test-preparing",
                    Duration::from_secs(1)
                )
                .await,
                Ok(0)
            ) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    let completed = if prepared.is_ok() {
        if shutdown_server {
            shutdown_tx.send(true).unwrap();
        } else {
            socket.send(ClientMessage::Close(None)).await.unwrap();
        }
        timeout(Duration::from_secs(2), done.notified())
            .await
            .is_ok()
    } else {
        false
    };
    let removed = matches!(
        docker.inspect_container(&name, None).await,
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
        })
    );
    let released = completed
        && timeout(
            Duration::from_secs(2),
            slots.acquire_many(MAX_SESSIONS as u32),
        )
        .await
        .is_ok();
    remove_sandbox(&docker, &name).await.unwrap();
    server.abort();
    assert!(prepared.is_ok(), "Slow lesson never reached preparation");
    assert!(
        completed,
        "Cancellation must not wait for the 30-second setup"
    );
    assert!(
        removed,
        "The prepared container must be removed before completion"
    );
    assert!(released, "Cancellation must release the session slot");
}

#[tokio::test]
#[ignore = "Requires Docker and a built sandbox image"]
async fn disconnect_during_preparation_removes_container() {
    cancel_slow_preparation(false).await;
}

#[tokio::test]
#[ignore = "Requires Docker and a built sandbox image"]
async fn shutdown_during_preparation_removes_container() {
    cancel_slow_preparation(true).await;
}

#[tokio::test]
async fn stalled_terminal_resize_releases_container_and_slot() {
    use tokio::io::AsyncReadExt;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let resize = Arc::new(Notify::new());
    let removed = Arc::new(Notify::new());
    let daemon_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let daemon_address = daemon_listener.local_addr().unwrap();
    let daemon = tokio::spawn({
        let resize = resize.clone();
        let removed = removed.clone();
        async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                let (stream, _) = daemon_listener.accept().await.unwrap();
                let resize = resize.clone();
                let removed = removed.clone();
                connections.spawn(async move {
                    let mut stream = BufReader::new(stream);
                    let mut request = String::new();
                    stream.read_line(&mut request).await.unwrap();
                    let path = request.split_whitespace().nth(1).unwrap().to_owned();
                    let mut length = 0;
                    loop {
                        let mut header = String::new();
                        stream.read_line(&mut header).await.unwrap();
                        if header == "\r\n" {
                            break;
                        }
                        if let Some(value) = header.to_lowercase().strip_prefix("content-length:") {
                            length = value.trim().parse().unwrap();
                        }
                    }
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).await.unwrap();
                    let (status, response) = if path.ends_with("/containers/create")
                        || path.contains("/containers/create?")
                    {
                        ("201 Created", r#"{"Id":"test-container","Warnings":[]}"#)
                    } else if path.ends_with("/exec") {
                        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
                        if body["Tty"] == true {
                            ("201 Created", r#"{"Id":"terminal"}"#)
                        } else {
                            ("201 Created", r#"{"Id":"setup"}"#)
                        }
                    } else if path.ends_with("/exec/setup/json") {
                        ("200 OK", r#"{"ExitCode":0}"#)
                    } else if path.contains("/exec/terminal/resize?") {
                        resize.notify_one();
                        // Simulate a Docker Engine that never replies to resize.
                        std::future::pending::<()>().await;
                        unreachable!()
                    } else if path.ends_with("/exec/setup/start")
                        || path.ends_with("/exec/terminal/start")
                    {
                        stream.get_mut().write_all(b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: tcp\r\n\r\n").await.unwrap();
                        if path.ends_with("/exec/terminal/start") {
                            std::future::pending::<()>().await;
                        }
                        return;
                    } else if request.starts_with("DELETE ") {
                        removed.notify_one();
                        ("204 No Content", "")
                    } else if path.ends_with("/start") {
                        ("204 No Content", "")
                    } else {
                        panic!("Unexpected Docker request: {request}");
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                        response.len()
                    );
                    stream.get_mut().write_all(response.as_bytes()).await.unwrap();
                });
            }
        }
    });
    let (_shutdown_tx, shutdown) = watch::channel(false);
    let slots = Arc::new(Semaphore::new(MAX_SESSIONS));
    let app = App {
        docker: Docker::connect_with_http(
            &format!("http://{daemon_address}"),
            120,
            bollard::API_DEFAULT_VERSION,
        )
        .unwrap(),
        image: "test-image".into(),
        origins: vec!["http://localhost".into()],
        slots: slots.clone(),
        shutdown,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/terminal/{lesson}", get(terminal))
        .with_state(app);
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let result = timeout(IO_TIMEOUT + Duration::from_secs(3), async {
        let mut request = format!("ws://{address}/terminal/mkdir")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("origin", "http://localhost".parse().unwrap());
        let (mut socket, _) = connect_async(request).await.unwrap();
        while let Some(message) = socket.next().await {
            if let ClientMessage::Text(text) = message.unwrap() {
                let event: serde_json::Value = serde_json::from_str(&text).unwrap();
                assert_eq!(event["type"], "ready", "{event}");
                break;
            }
        }
        socket
            .send(ClientMessage::Text(
                json!({"type":"resize", "cols":100, "rows":30})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        resize.notified().await;
        removed.notified().await;
        let _permit = slots.acquire_many(MAX_SESSIONS as u32).await.unwrap();
    })
    .await;
    server.abort();
    daemon.abort();
    assert!(
        result.is_ok(),
        "A stalled resize must not keep the container and session slot for Docker's 120-second timeout"
    );
}
