use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result};
use bollard::{
    Docker,
    exec::{CreateExecOptions, StartExecResults},
    query_parameters::{ListContainersOptionsBuilder, RemoveContainerOptionsBuilder},
};
use futures_util::StreamExt;
use tokio::time::timeout;

use super::config::sandbox::SESSION_LABEL;

pub(super) async fn run_command(
    docker: &Docker,
    container: &str,
    command: &str,
    limit: Duration,
) -> Result<i64> {
    timeout(limit + Duration::from_secs(2), async {
        let exec = docker
            .create_exec(
                container,
                CreateExecOptions {
                    attach_stdout: Some(true),
                    attach_stderr: Some(true),
                    cmd: Some(vec![
                        "/usr/bin/timeout".to_owned(),
                        "--kill-after=1s".to_owned(),
                        format!("{}s", limit.as_secs()),
                        "/bin/sh".to_owned(),
                        "-c".to_owned(),
                        command.to_owned(),
                    ]),
                    env: Some(vec!["PATH=/usr/sbin:/usr/bin:/sbin:/bin".to_owned()]),
                    ..Default::default()
                },
            )
            .await?;
        if let StartExecResults::Attached { mut output, .. } =
            docker.start_exec(&exec.id, None).await?
        {
            while let Some(chunk) = output.next().await {
                chunk?;
            }
        }
        docker
            .inspect_exec(&exec.id)
            .await?
            .exit_code
            .context("Command did not exit")
    })
    .await
    .context("Docker command timed out")?
}

pub(super) async fn remove_sandbox(docker: &Docker, name: &str) -> Result<()> {
    match docker
        .remove_container(
            name,
            Some(
                RemoveContainerOptionsBuilder::default()
                    .force(true)
                    .v(true)
                    .build(),
            ),
        )
        .await
    {
        Ok(()) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404, ..
        }) => Ok(()),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 409,
            message,
        }) if message.contains("removal") && message.contains("already in progress") => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn cleanup_sandboxes(docker: &Docker) -> Result<()> {
    let options = ListContainersOptionsBuilder::default()
        .all(true)
        .filters(&HashMap::from([(
            "label",
            vec![format!("{SESSION_LABEL}=true")],
        )]))
        .build();
    for container in docker.list_containers(Some(options)).await? {
        if let Some(id) = container.id {
            remove_sandbox(docker, &id).await?;
        }
    }
    Ok(())
}
