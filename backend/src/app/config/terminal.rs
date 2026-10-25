use std::time::Duration;

use bollard::exec::CreateExecOptions;

use crate::lessons::Lesson;

pub(crate) const MAX_SESSIONS: usize = 4;
pub(crate) const IO_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
pub(crate) const SETUP_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const CHECK_TIMEOUT: Duration = Duration::from_secs(4);
pub(crate) const CHECK_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);
pub(crate) const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
pub(crate) const SESSION_TIMEOUT: Duration = Duration::from_secs(60 * 60);
pub(crate) const MAX_MESSAGE_SIZE: usize = 16 * 1024;
pub(crate) const MAX_COLS: u16 = 500;
pub(crate) const MAX_ROWS: u16 = 200;

pub(crate) fn shell(lesson: &Lesson) -> CreateExecOptions<&str> {
    let shell_path = if lesson.lab_model {
        "PATH=/opt/linuxcatch/lab/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    } else {
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    };
    CreateExecOptions {
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        tty: Some(true),
        cmd: Some(vec![
            "/bin/bash",
            "--noprofile",
            "--rcfile",
            "/etc/linuxcatch/bashrc",
            "-i",
        ]),
        env: Some(vec![
            "PS1=\\[\\e[38;5;114m\\]student@linuxcatch\\[\\e[0m\\]:\\[\\e[38;5;110m\\]\\w\\[\\e[0m\\]$ ",
            "PROMPT_COMMAND=builtin history -a; builtin pwd > /tmp/linuxcatch.cwd",
            shell_path,
            "LINUXCATCH_LAB_USER=student",
        ]),
        working_dir: Some(lesson.start_dir.as_str()),
        ..Default::default()
    }
}
