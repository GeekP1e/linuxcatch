use std::time::Duration;

use anyhow::{Context, Result, bail};
use axum::{
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bollard::{
    exec::{ResizeExecOptions, StartExecOptions, StartExecResults},
    query_parameters::{CreateContainerOptionsBuilder, StartContainerOptions},
};
use futures_util::{SinkExt, StreamExt, future::BoxFuture};
use serde::Deserialize;
use serde_json::json;
use tokio::{
    io::AsyncWriteExt,
    time::{Instant, timeout},
};
use uuid::Uuid;

use super::{
    config::{sandbox, terminal::*},
    sandbox::{remove_sandbox, run_command},
    state::App,
};
use crate::lessons;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Control {
    Resize { cols: u16, rows: u16 },
    Check,
}

pub(super) async fn terminal(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let origin = headers.get("origin").and_then(|value| value.to_str().ok());
    if !origin.is_some_and(|value| app.origins.iter().any(|allowed| allowed == value)) {
        return (StatusCode::FORBIDDEN, "Origin is not allowed").into_response();
    }
    let Some(lesson) = lessons::find(&id) else {
        return (StatusCode::NOT_FOUND, "Unknown lesson").into_response();
    };
    let Ok(permit) = app.slots.clone().try_acquire_owned() else {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "Maximum of four sessions reached",
        )
            .into_response();
    };
    ws.max_message_size(MAX_MESSAGE_SIZE).on_upgrade(move |mut socket| async move {
        let _permit = permit;
        let name = format!("linuxcatch-session-{}", Uuid::new_v4());
        if let Err(error) = run_session(&app, &name, &lesson, &mut socket).await {
            tracing::warn!(session = %name, %error, "Сессия завершилась с ошибкой");
            let _ = event(&mut socket, json!({"type": "error", "message": lessons::Localized::new("Ошибка учебного окружения. Проверь Docker и попробуй сбросить терминал.", "The training environment encountered an error. Check Docker and reset the terminal.")})).await;
        }
        if let Err(error) = remove_sandbox(&app.docker, &name).await {
            tracing::warn!(session = %name, %error, "Не удалось удалить окружение");
        }
        let _ = timeout(CLOSE_TIMEOUT, socket.close()).await;
    }).into_response()
}

pub(super) async fn run_session(
    app: &App,
    name: &str,
    lesson: &lessons::Lesson,
    socket: &mut WebSocket,
) -> Result<()> {
    let mut shutdown = app.shutdown.clone();
    if *shutdown.borrow() {
        return Ok(());
    }
    let config = sandbox::container(&app.image, lesson.large_workspace);
    app.docker
        .create_container(
            Some(CreateContainerOptionsBuilder::default().name(name).build()),
            config,
        )
        .await?;
    // Finish creation before allowing cancellation, so cleanup knows this container exists.
    let (exec_id, attached) = {
        let prepare = async {
            app.docker
                .start_container(name, None::<StartContainerOptions>)
                .await?;
            let setup = format!(
                "set -eu; : > /tmp/linuxcatch.bashrc; mkdir -p /workspace/inbox; printf 'Welcome to LinuxCatch!\\nLearn by doing.\\n' > /workspace/inbox/welcome.txt; {} {}",
                if lesson.lab_model {
                    "/usr/local/bin/linuxcatch-lab init;"
                } else {
                    ""
                },
                if lesson.setup.is_empty() {
                    ":"
                } else {
                    lesson.setup.as_str()
                }
            );
            if run_command(&app.docker, name, &setup, SETUP_TIMEOUT).await? != 0 {
                bail!("Lesson setup failed");
            }
            let exec = app.docker.create_exec(name, shell(lesson)).await?;
            let attached = app
                .docker
                .start_exec(
                    &exec.id,
                    Some(StartExecOptions {
                        tty: true,
                        ..Default::default()
                    }),
                )
                .await?;
            Ok::<_, anyhow::Error>((exec.id, attached))
        };
        tokio::pin!(prepare);
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }
            tokio::select! {
                _ = shutdown.changed() => return Ok(()),
                result = &mut prepare => break result?,
                message = socket.next() => match message {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return Ok(()),
                    Some(Ok(Message::Ping(bytes))) => send(socket, Message::Pong(bytes)).await?,
                    _ => {},
                },
            }
        }
    };
    let StartExecResults::Attached {
        mut output,
        mut input,
    } = attached
    else {
        bail!("Terminal did not attach");
    };
    event(socket, json!({"type": "ready", "lesson": lesson.id})).await?;
    let started = Instant::now();
    let mut last_activity = Instant::now();
    let mut last_check = Instant::now() - Duration::from_secs(1);
    let mut check_future: Option<BoxFuture<'static, Result<i64>>> = None;
    let mut timer = tokio::time::interval(HEARTBEAT_INTERVAL);
    loop {
        if *shutdown.borrow() {
            break;
        }
        tokio::select! {
            _ = shutdown.changed() => break,
            result = output.next() => {
                match result {
                    Some(Ok(chunk)) => send(socket, Message::Binary(chunk.into_bytes())).await?,
                    Some(Err(error)) => return Err(error.into()),
                    None => break,
                }
            },
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Binary(bytes))) => {
                        last_activity = Instant::now();
                        timeout(IO_TIMEOUT, async {
                            input.write_all(&bytes).await?;
                            input.flush().await
                        }).await.context("Terminal input stalled")??;
                    },
                    Some(Ok(Message::Text(text))) => {
                        last_activity = Instant::now();
                        match serde_json::from_str::<Control>(&text) {
                            Ok(Control::Resize { cols, rows }) if cols > 0 && rows > 0 && cols <= MAX_COLS && rows <= MAX_ROWS => {
                                timeout(IO_TIMEOUT, app.docker.resize_exec(&exec_id, ResizeExecOptions { width: cols, height: rows }))
                                    .await.context("Terminal resize stalled")??;
                            },
                            Ok(Control::Check) if check_future.is_none() => {
                                let delay = CHECK_INTERVAL.saturating_sub(last_check.elapsed());
                                let docker = app.docker.clone();
                                let container = name.to_owned();
                                let command = lesson.check.clone();
                                check_future = Some(Box::pin(async move {
                                    tokio::time::sleep(delay).await;
                                    run_command(&docker, &container, &command, CHECK_TIMEOUT).await
                                }));
                            },
                            _ => {},
                        }
                    },
                    Some(Ok(Message::Ping(bytes))) => send(socket, Message::Pong(bytes)).await?,
                    Some(Ok(Message::Pong(_))) => {},
                    _ => break,
                }
            },
            result = async {
                match check_future.as_mut() {
                    Some(check) => check.await,
                    None => std::future::pending().await,
                }
            } => {
                check_future = None;
                last_check = Instant::now();
                let exit_code = result?;
                let passed = exit_code == 0;
                let message = if passed {
                    lesson.success.clone()
                } else if matches!(exit_code, 124 | 137) {
                    lessons::Localized::new("Проверка превысила лимит времени. Проверь файлы задания и попробуй снова.", "The check exceeded its time limit. Check the task files and try again.")
                } else {
                    lessons::Localized::new("Цель ещё не достигнута. Сверь результат с заданием и проверь нужные имена, пути и содержимое файлов.", "The objective is not complete yet. Compare your result with the task and check the required names, paths, and file contents.")
                };
                event(socket, json!({"type": "checked", "passed": passed, "message": message})).await?;
            },
            _ = timer.tick() => {
                if last_activity.elapsed() > IDLE_TIMEOUT || started.elapsed() > SESSION_TIMEOUT {
                    event(socket, json!({"type": "expired", "message": lessons::Localized::new("Время сессии истекло. Сбрось терминал, чтобы продолжить.", "This session has expired. Reset the terminal to continue.")})).await?;
                    break;
                }
                send(socket, Message::Ping(Vec::new().into())).await?;
            }
        }
    }
    Ok(())
}

async fn event(socket: &mut WebSocket, value: serde_json::Value) -> Result<()> {
    send(socket, Message::Text(value.to_string().into())).await
}

async fn send(socket: &mut WebSocket, message: Message) -> Result<()> {
    timeout(IO_TIMEOUT, socket.send(message))
        .await
        .context("WebSocket output stalled")??;
    Ok(())
}
