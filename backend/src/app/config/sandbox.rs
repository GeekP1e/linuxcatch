use std::collections::HashMap;

use bollard::models::{ContainerCreateBody, HostConfig};

pub(crate) const SESSION_LABEL: &str = "io.linuxcatch.sandbox";

pub(crate) fn image() -> String {
    std::env::var("SANDBOX_IMAGE").unwrap_or_else(|_| "linuxcatch-sandbox:local".into())
}

pub(crate) fn container(image: &str, large_workspace: bool) -> ContainerCreateBody {
    ContainerCreateBody {
        image: Some(image.to_owned()),
        user: Some("1000:1000".into()),
        working_dir: Some("/workspace".into()),
        env: Some(vec![
            "TERM=xterm-256color".into(),
            "LANG=C.UTF-8".into(),
            "HISTFILE=/tmp/linuxcatch.history".into(),
            "HISTSIZE=1000".into(),
            "HISTCONTROL=".into(),
        ]),
        labels: Some(HashMap::from([(SESSION_LABEL.into(), "true".into())])),
        host_config: Some(HostConfig {
            init: Some(true),
            network_mode: Some("none".into()),
            readonly_rootfs: Some(true),
            cap_drop: Some(vec!["ALL".into()]),
            security_opt: Some(vec!["no-new-privileges:true".into()]),
            memory: Some(if large_workspace { 384 } else { 128 } * 1024 * 1024),
            memory_swap: Some(if large_workspace { 384 } else { 128 } * 1024 * 1024),
            nano_cpus: Some(500_000_000),
            pids_limit: Some(64),
            tmpfs: Some(HashMap::from([
                (
                    "/home/student".into(),
                    "rw,nosuid,nodev,size=16m,uid=1000,gid=1000".into(),
                ),
                (
                    "/workspace".into(),
                    if large_workspace {
                        "rw,nosuid,nodev,size=192m,uid=1000,gid=1000"
                    } else {
                        "rw,nosuid,nodev,size=32m,uid=1000,gid=1000"
                    }
                    .into(),
                ),
                ("/tmp".into(), "rw,nosuid,nodev,size=16m,mode=1777".into()),
            ])),
            ..Default::default()
        }),
        ..Default::default()
    }
}
