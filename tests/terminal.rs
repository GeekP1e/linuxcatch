use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[test]
fn occupied_port_is_rejected_before_docker_access_or_cleanup() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_linuxcatch-backend"))
        .env("LISTEN_ADDR", address.to_string())
        .env("DOCKER_HOST", "unix:///linuxcatch-test-missing.sock")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains(&format!("Cannot bind {address}")), "{error}");
}

async fn connect(lesson: &str) -> Socket {
    let base =
        std::env::var("LINUXCATCH_TEST_URL").unwrap_or_else(|_| "ws://localhost:8080".into());
    let mut request = format!("{base}/api/terminal/{lesson}")
        .into_client_request()
        .unwrap();
    let origin = base.replacen("ws://", "http://", 1);
    request
        .headers_mut()
        .insert("origin", origin.parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let ready = next_event(&mut socket, "ready").await;
    assert_eq!(ready["lesson"], lesson);
    socket
        .send(Message::Text(
            json!({"type": "resize", "cols": 120, "rows": 32})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    socket
}

async fn next_event(socket: &mut Socket, kind: &str) -> Value {
    timeout(Duration::from_secs(15), async {
        while let Some(message) = socket.next().await {
            match message.unwrap() {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(value["type"], "error", "{value}");
                    if value["type"] == kind {
                        return value;
                    }
                }
                Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
                _ => {}
            }
        }
        panic!("Socket closed before {kind}");
    })
    .await
    .expect("Server event timed out")
}

async fn command(socket: &mut Socket, command: &str) -> String {
    socket
        .send(Message::Binary(
            format!("{command}; printf '\\nLC_DONE\\n'\r")
                .into_bytes()
                .into(),
        ))
        .await
        .unwrap();
    timeout(Duration::from_secs(10), async {
        let mut output = String::new();
        while let Some(message) = socket.next().await {
            match message.unwrap() {
                Message::Binary(bytes) => {
                    output.push_str(&String::from_utf8_lossy(&bytes));
                    if output.contains("\r\nLC_DONE\r\n") {
                        return output;
                    }
                }
                Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
                _ => {}
            }
        }
        panic!("Socket closed during command");
    })
    .await
    .expect("Command timed out")
}

async fn check(socket: &mut Socket) -> bool {
    socket
        .send(Message::Text(json!({"type": "check"}).to_string().into()))
        .await
        .unwrap();
    next_event(socket, "checked").await["passed"]
        .as_bool()
        .unwrap()
}

async fn wait_output(socket: &mut Socket, needle: &str) -> String {
    timeout(Duration::from_secs(5), async {
        let mut output = String::new();
        while let Some(message) = socket.next().await {
            match message.unwrap() {
                Message::Binary(bytes) => {
                    output.push_str(&String::from_utf8_lossy(&bytes));
                    if output.contains(needle) {
                        return output;
                    }
                }
                Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
                _ => {}
            }
        }
        panic!("Socket closed before {needle}");
    })
    .await
    .unwrap_or_else(|_| panic!("Terminal output timed out waiting for {needle}"))
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn real_terminal_checks_state_and_isolates_sessions() {
    let mut first = connect("mkdir").await;
    let mut second = connect("mkdir").await;
    assert!(!check(&mut first).await);
    command(&mut first, "install -d /workspace/notes").await;
    // A different command must satisfy the same filesystem objective.
    assert!(check(&mut first).await);
    assert!(!check(&mut second).await);

    first
        .send(Message::Text(
            json!({"type": "resize", "cols": 101, "rows": 33})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let output = command(&mut first, "stty size; printf 'one\\ntwo\\n' | grep two").await;
    assert!(output.contains("33 101"), "{output}");
    assert!(output.contains("\r\ntwo\r\n"), "{output}");
    let output = command(
        &mut first,
        "id -u; test ! -S /var/run/docker.sock && echo NO_SOCKET",
    )
    .await;
    assert!(output.contains("\r1000\r\n"), "{output}");
    assert!(output.contains("\r\nNO_SOCKET\r\n"), "{output}");
    first
        .send(Message::Binary(
            b"sh -c 'printf \"\\nWAITING\\n\"; exec sleep 30'\r"
                .to_vec()
                .into(),
        ))
        .await
        .unwrap();
    wait_output(&mut first, "\r\nWAITING\r\n").await;
    first.send(Message::Binary(vec![3].into())).await.unwrap();
    wait_output(&mut first, "student@linuxcatch").await;
    let output = command(&mut first, "echo INTERRUPTED").await;
    assert!(output.contains("\rINTERRUPTED\r\n"), "{output}");
    first.close(None).await.unwrap();
    second.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn exited_background_processes_are_reaped() {
    let mut socket = connect("mkdir").await;
    let output = command(
        &mut socket,
        "for i in {1..20}; do sh -c 'sleep 0.1 &'; done; sleep 1; ps -eo stat= | awk '$1 ~ /^Z/ {n++} END {print \"ZOMBIES=\" (n+0)}'",
    )
    .await;
    socket.close(None).await.unwrap();
    assert!(output.contains("ZOMBIES=0\r\n"), "{output}");
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn practice_requires_all_objectives_and_original_content() {
    let mut socket = connect("organize").await;
    assert!(!check(&mut socket).await);
    command(
        &mut socket,
        "mkdir notes; mv inbox/welcome.txt notes/; touch notes/plan.txt",
    )
    .await;
    assert!(check(&mut socket).await);
    command(&mut socket, "printf 'wrong' > notes/welcome.txt").await;
    assert!(!check(&mut socket).await);
    socket.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn lessons_reject_wrong_content_link_targets_attributes_and_devices() {
    let cases = [
        (
            "glob",
            "printf 'wrong' > notes/a.txt; touch notes/b.txt",
            "cp source/*.txt notes/",
        ),
        (
            "cp-archive",
            "cp -a docs archive; ln -sfn missing.txt archive/link.txt",
            "ln -sfn file.txt archive/link.txt",
        ),
        (
            "ln-dangling",
            "ln -s missing.txt shortcut.txt; rm original.txt",
            "ln -sfn /workspace/original.txt shortcut.txt",
        ),
        (
            "practice-options",
            "cp -a docs backup; ln -sfn missing.txt backup/link.txt",
            "ln -sfn file.txt backup/link.txt",
        ),
        (
            "practice-options",
            "cp -a docs backup; touch -d '2001-01-01 UTC' backup/file.txt",
            "touch -r docs/file.txt backup/file.txt",
        ),
        (
            "parted",
            "parted -s disk.img mklabel gpt mkpart primary 2MiB 18MiB",
            "parted -s disk.img mklabel gpt mkpart primary 1MiB 17MiB",
        ),
        (
            "fstab-options",
            "printf '/dev/sdb1 /home ext4 defaults,noatime,noexec 0 2\\n' > lab/etc/fstab; sudo mount /home",
            "printf '/dev/sdc2 /home ext4 defaults,noatime,noexec 0 2\\n' > lab/etc/fstab; sudo mount /home",
        ),
    ];
    let mut failures = Vec::new();
    for (id, invalid, repair) in cases {
        let mut socket = connect(id).await;
        command(&mut socket, invalid).await;
        if check(&mut socket).await {
            failures.push(format!("{id} accepted: {invalid}"));
        }
        command(&mut socket, repair).await;
        if !check(&mut socket).await {
            failures.push(format!("{id} rejected repair: {repair}"));
        }
        socket.close(None).await.unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn reports_must_match_the_requested_file_and_partition_state() {
    let course: Value =
        serde_json::from_str(include_str!("../backend/content/course.json")).unwrap();
    let cases = [
        ("ls-inodes", "printf '0 note.txt\\n' > inode.txt"),
        (
            "blkid-image",
            "blkid -p partition.img | sed -E 's/UUID=\"[^\"]*\"/UUID=\"wrong\"/' > identifiers.txt",
        ),
        (
            "practice-filesystems",
            "mkfs.ext4 -q -F partition.img; blkid -p partition.img | sed -E 's/UUID=\"[^\"]*\"/UUID=\"wrong\"/' > identifiers.txt; fsck -n /workspace/partition.img > check.txt 2>&1; echo \"$?\" > status.txt",
        ),
        (
            "practice-filesystems",
            "mkfs.ext4 -q -F partition.img; blkid -p partition.img | sed -E 's/ TYPE=\"[^\"]*\"//' > identifiers.txt; fsck -n /workspace/partition.img > check.txt 2>&1; echo \"$?\" > status.txt",
        ),
        (
            "fdisk-discard",
            "parted -s disk.img mklabel gpt mkpart primary 2MiB 18MiB; printf 'd\\np\\nq\\n' | fdisk disk.img > preview.txt",
        ),
        (
            "fdisk-discard",
            "printf 'not a partition report\\n' > preview.txt",
        ),
        ("info", "info --help > /tmp/info-help.txt"),
        (
            "mount-all",
            "sudo mount /dev/sdb1 /home; sudo mount /dev/sdc2 /mnt",
        ),
    ];
    let mut failures = Vec::new();
    for (id, invalid) in cases {
        let mut socket = connect(id).await;
        command(&mut socket, invalid).await;
        if check(&mut socket).await {
            failures.push(format!("{id} accepted: {invalid}"));
        }
        // Restore the original disk before testing the discard exercise's solution.
        if id == "fdisk-discard" {
            command(
                &mut socket,
                "parted -s disk.img mklabel gpt mkpart primary 1MiB 17MiB",
            )
            .await;
        }
        let lesson = course["lessons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|lesson| lesson["id"] == id)
            .unwrap();
        command(&mut socket, lesson["solution"].as_str().unwrap()).await;
        if !check(&mut socket).await {
            failures.push(format!("{id} rejected its solution"));
        }
        socket.close(None).await.unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn blocked_check_keeps_terminal_responsive_and_can_be_retried() {
    let mut socket = connect("alias-list").await;
    command(&mut socket, "mkfifo aliases.txt").await;
    socket
        .send(Message::Text(json!({"type": "check"}).to_string().into()))
        .await
        .unwrap();
    socket
        .send(Message::Binary(
            b"printf '\\nINPUT_DURING_CHECK\\n'\r".to_vec().into(),
        ))
        .await
        .unwrap();
    timeout(
        Duration::from_secs(2),
        wait_output(&mut socket, "\r\nINPUT_DURING_CHECK\r\n"),
    )
    .await
    .expect("A pending check must not block terminal input or output");
    let result = next_event(&mut socket, "checked").await;
    assert_eq!(result["passed"], false);
    assert!(
        result["message"]["en"]
            .as_str()
            .unwrap()
            .contains("time limit")
    );
    let output = command(
        &mut socket,
        "if pgrep -f '^(grep|/bin/sh -c grep) -q.* /workspace/aliases.txt$' >/dev/null; then echo CHECKER_LEAKED; else echo CHECKER_STOPPED; fi",
    ).await;
    assert!(output.contains("CHECKER_STOPPED\r\n"), "{output}");
    command(&mut socket, "rm aliases.txt; alias > aliases.txt").await;
    assert!(check(&mut socket).await);
    socket.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn disconnect_during_check_releases_the_session_slot() {
    for _ in 0..6 {
        let mut socket = connect("alias-list").await;
        command(&mut socket, "mkfifo aliases.txt").await;
        socket
            .send(Message::Text(json!({"type": "check"}).to_string().into()))
            .await
            .unwrap();
        socket.close(None).await.unwrap();
    }
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn repeated_checks_are_rate_limited() {
    let mut socket = connect("mkdir").await;
    let started = tokio::time::Instant::now();
    for _ in 0..3 {
        assert!(!check(&mut socket).await);
    }
    assert!(started.elapsed() >= Duration::from_millis(900));
    socket.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn rejects_foreign_origins() {
    let base =
        std::env::var("LINUXCATCH_TEST_URL").unwrap_or_else(|_| "ws://localhost:8080".into());
    let mut request = format!("{base}/api/terminal/mkdir")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("origin", "https://untrusted.example".parse().unwrap());
    match connect_async(request).await.unwrap_err() {
        tokio_tungstenite::tungstenite::Error::Http(response) => assert_eq!(response.status(), 403),
        error => panic!("Unexpected error: {error}"),
    }
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn every_course_fixture_starts_incomplete_and_accepts_its_solution() {
    let course: Value =
        serde_json::from_str(include_str!("../backend/content/course.json")).unwrap();
    for lesson in course["lessons"].as_array().unwrap() {
        let id = lesson["id"].as_str().unwrap();
        if let Ok(groups) = std::env::var("LINUXCATCH_TEST_GROUPS")
            && !groups.split(',').any(|group| lesson["group"] == group)
        {
            continue;
        }
        eprintln!("Checking lesson {id}");
        let mut socket = connect(id).await;
        assert!(!check(&mut socket).await, "{id} starts completed");
        let output = command(&mut socket, lesson["solution"].as_str().unwrap()).await;
        assert!(
            check(&mut socket).await,
            "{id} rejected its solution: {output}"
        );
        socket.close(None).await.unwrap();
    }
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn administration_model_is_isolated_and_keeps_real_permissions() {
    let mut first = connect("useradd").await;
    let mut second = connect("useradd").await;
    let output = command(&mut first, "sudo useradd user2; sudo whoami; /usr/bin/id -u; test ! -w /etc/passwd && echo READONLY; test ! -b /dev/sdb && echo NO_DISK").await;
    assert!(output.contains("\rroot\r\n"), "{output}");
    assert!(output.contains("\r\n1000\r\n"), "{output}");
    assert!(output.contains("\r\nREADONLY\r\n"), "{output}");
    assert!(output.contains("\r\nNO_DISK\r\n"), "{output}");
    assert!(check(&mut first).await);
    assert!(!check(&mut second).await);
    let output = command(
        &mut first,
        "printf 'wrong\\n' | su -c 'echo BAD_AUTH' root; echo AUTH_STATUS=$?",
    )
    .await;
    assert!(output.contains("AUTH_STATUS=1"), "{output}");
    first.close(None).await.unwrap();
    second.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn administration_model_preserves_concurrent_updates() {
    let mut socket = connect("useradd").await;
    let output = command(
        &mut socket,
        "seq 1 12 | xargs -P4 -I{} sudo useradd parallel{}; echo ADD_STATUS=$?",
    )
    .await;
    assert!(output.contains("ADD_STATUS=0\r\n"), "{output}");
    let output = command(
        &mut socket,
        "python3 -c 'import json; from pathlib import Path; root = Path(\"/workspace/lab\"); users = json.loads((root / \"state.json\").read_text())[\"users\"]; assert all(\"parallel\" + str(i) in users for i in range(1, 13)); assert len({u[\"uid\"] for u in users.values()}) == len(users); assert all({line.split(\":\")[0] for line in (root / name).read_text().splitlines()} == set(users) for name in [\"etc/passwd\", \"etc/shadow\"]); print(\"ALL_USERS_SAVED\")'",
    )
    .await;
    assert!(output.contains("ALL_USERS_SAVED\r\n"), "{output}");
    socket.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn administration_model_returns_shell_signal_status() {
    let mut socket = connect("useradd").await;
    let output = command(
        &mut socket,
        "sudo sh -c 'kill -TERM $$'; echo SUDO_STATUS=$?; su -c 'kill -TERM $$' student; echo SU_STATUS=$?",
    )
    .await;
    assert!(output.contains("SUDO_STATUS=143\r\n"), "{output}");
    assert!(output.contains("SU_STATUS=143\r\n"), "{output}");
    socket.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn administration_model_interrupt_has_no_traceback() {
    let mut socket = connect("useradd").await;
    socket.send(Message::Binary(
        b"sudo sh -c 'printf \"\\nMODEL_SLEEPING\\n\"; exec sleep 30'; echo INTERRUPT_STATUS=$?\r".to_vec().into(),
    )).await.unwrap();
    wait_output(&mut socket, "\r\nMODEL_SLEEPING\r\n").await;
    socket.send(Message::Binary(vec![3].into())).await.unwrap();
    let output = wait_output(&mut socket, "INTERRUPT_STATUS=130\r\n").await;
    assert!(!output.contains("Traceback"), "{output}");
    assert!(!output.contains("KeyboardInterrupt"), "{output}");
    let output = command(&mut socket, "sudo whoami; /usr/bin/id -u").await;
    assert!(output.contains("root\r\n"), "{output}");
    assert!(output.contains("1000\r\n"), "{output}");
    socket
        .send(Message::Binary(
            b"sudo passwd student; echo PASSWORD_STATUS=$?\r"
                .to_vec()
                .into(),
        ))
        .await
        .unwrap();
    wait_output(&mut socket, "New password:").await;
    socket.send(Message::Binary(vec![3].into())).await.unwrap();
    let output = wait_output(&mut socket, "PASSWORD_STATUS=130\r\n").await;
    assert!(!output.contains("Traceback"), "{output}");
    command(&mut socket, "sudo useradd user2").await;
    assert!(
        check(&mut socket).await,
        "An interrupted passwd must release its lock"
    );
    socket.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn administration_model_su_keeps_its_parent_after_interrupt() {
    let mut socket = connect("useradd").await;
    socket
        .send(Message::Binary(b"su root\r".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut socket, "Password:").await;
    socket
        .send(Message::Binary(b"rootpass\r".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut socket, "root@linuxcatch").await;
    socket
        .send(Message::Binary(
            b"printf '\\nCHILD_SLEEPING\\n'; sleep 30\r".to_vec().into(),
        ))
        .await
        .unwrap();
    wait_output(&mut socket, "\r\nCHILD_SLEEPING\r\n").await;
    socket.send(Message::Binary(vec![3].into())).await.unwrap();
    let output = wait_output(&mut socket, "root@linuxcatch").await;
    assert!(!output.contains("Traceback"), "{output}");
    let output = command(&mut socket,
        "if ps -o args= -p \"$PPID\" | grep -q '/opt/linuxcatch/lab/bin/su root'; then echo SU_PARENT_ALIVE; fi; whoami",
    ).await;
    assert!(output.contains("SU_PARENT_ALIVE\r\n"), "{output}");
    assert!(output.contains("root\r\n"), "{output}");
    socket
        .send(Message::Binary(b"exit\r".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut socket, "student@linuxcatch").await;
    let output = command(&mut socket, "whoami").await;
    assert!(output.contains("student\r\n"), "{output}");
    socket.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn administration_model_rejects_extra_arguments_without_side_effects() {
    let mut socket = connect("useradd").await;
    let output = command(&mut socket,
        "sudo useradd extra unwanted; echo USERADD_STATUS=$?; printf 'wrong\\nwrong\\n' | sudo passwd student extra; echo PASSWD_STATUS=$?; printf 'rootpass\\n' | su -c true student root; echo SU_STATUS=$?",
    ).await;
    for name in ["USERADD", "PASSWD", "SU"] {
        assert!(output.contains(&format!("{name}_STATUS=1\r\n")), "{output}");
    }
    let output = command(&mut socket,
        "python3 -c 'import json,hashlib; from pathlib import Path; users=json.loads(Path(\"/workspace/lab/state.json\").read_text())[\"users\"]; assert set(users)=={\"root\",\"student\"}; assert users[\"student\"][\"password\"]==hashlib.sha256(b\"studentpass\").hexdigest(); print(\"STATE_UNCHANGED\")'",
    ).await;
    assert!(output.contains("STATE_UNCHANGED\r\n"), "{output}");
    socket.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn training_visudo_edits_and_validates_through_nano() {
    let mut socket = connect("sudo-visudo").await;
    assert!(!check(&mut socket).await);
    socket
        .send(Message::Binary(b"sudo visudo\r".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut socket, "GNU nano").await;
    socket
        .send(Message::Binary(
            b"\x1b/user2 ALL=(ALL) NOPASSWD: ALL\x0f".to_vec().into(),
        ))
        .await
        .unwrap();
    wait_output(&mut socket, "File Name to Write").await;
    socket
        .send(Message::Binary(b"\r".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut socket, "Wrote").await;
    socket.send(Message::Binary(vec![24].into())).await.unwrap();
    wait_output(&mut socket, "student@linuxcatch").await;
    assert!(check(&mut socket).await);
    socket.close(None).await.unwrap();
}

#[tokio::test]
#[ignore = "Requires running LinuxCatch and Docker"]
async fn editors_save_through_the_real_terminal() {
    let mut nano = connect("nano-edit").await;
    nano.send(Message::Binary(b"nano note.txt\r".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut nano, "GNU nano").await;
    nano.send(Message::Binary(b"Hello Linux\x0f".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut nano, "File Name to Write").await;
    nano.send(Message::Binary(b"\r".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut nano, "Wrote").await;
    nano.send(Message::Binary(vec![24].into())).await.unwrap();
    wait_output(&mut nano, "student@linuxcatch").await;
    let saved = command(&mut nano, "od -An -tx1 note.txt").await;
    assert!(check(&mut nano).await, "Nano saved: {saved}");
    nano.close(None).await.unwrap();

    let mut vim = connect("vim-insert").await;
    vim.send(Message::Binary(b"vi edit.txt\r".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut vim, "edit.txt").await;
    vim.send(Message::Binary(b"IHello \x1b:wq\r".to_vec().into()))
        .await
        .unwrap();
    wait_output(&mut vim, "student@linuxcatch").await;
    assert!(check(&mut vim).await);
    vim.close(None).await.unwrap();
}
