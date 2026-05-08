use std::{
    process::Stdio,
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};

const MAX_LINE_BYTES: usize = 256 * 1024;

use super::{
    applet::{Config, Input},
    protocol::{ChildCommand, InitPayload, PanelCommand, parse_child_line},
};

const STDERR_LOG_WINDOW: Duration = Duration::from_secs(10);
const STDERR_LOG_LIMIT: usize = 20;

#[derive(Debug)]
pub enum Control {
    Restart,
    Reconfigure(Config),
}

pub async fn run(
    name: String,
    mut config: Config,
    mut outbound_rx: mpsc::Receiver<PanelCommand>,
    mut control_rx: mpsc::UnboundedReceiver<Control>,
    out: relm4::Sender<Input>,
) {
    loop {
        let Some(program) = config.command.first().cloned() else {
            tracing::warn!(applet = %name, "exec applet command is empty");
            let _ = out.send(Input::ChildExited);
            return;
        };

        tracing::info!(applet = %name, program = %program, "exec applet spawning child");
        let mut command_builder = Command::new(&program);
        command_builder
            .args(config.command.iter().skip(1))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if config.env_clear {
            command_builder.env_clear();
        }
        for (key, value) in &config.env {
            command_builder.env(key, value);
        }
        let mut child = match command_builder.spawn() {
            Ok(child) => child,
            Err(error) => {
                tracing::warn!(%error, applet = %name, "exec applet failed to spawn child");
                let _ = out.send(Input::ChildExited);
                tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
                continue;
            }
        };

        let Some(mut stdin) = child.stdin.take() else {
            tracing::warn!(applet = %name, "exec applet child has no stdin");
            let _ = out.send(Input::ChildExited);
            let _ = child.kill().await;
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
            continue;
        };

        let Some(stdout) = child.stdout.take() else {
            tracing::warn!(applet = %name, "exec applet child has no stdout");
            let _ = out.send(Input::ChildExited);
            let _ = child.kill().await;
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
            continue;
        };

        let Some(stderr) = child.stderr.take() else {
            tracing::warn!(applet = %name, "exec applet child has no stderr");
            let _ = out.send(Input::ChildExited);
            let _ = child.kill().await;
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
            continue;
        };

        if let Err(error) = write_panel_command(
            &mut stdin,
            &PanelCommand::Init(InitPayload {
                instance: name.clone(),
                options: config.options.clone(),
            }),
        )
        .await
        {
            tracing::warn!(%error, applet = %name, "exec applet failed to send init");
        }

        let mut stdout_lines = BoundedLines::new(BufReader::new(stdout), MAX_LINE_BYTES);
        let mut stderr_lines = BoundedLines::new(BufReader::new(stderr), MAX_LINE_BYTES);
        let mut stderr_open = true;
        let mut stderr_limiter = StderrLogLimiter::default();

        let exit = loop {
            tokio::select! {
                control = control_rx.recv() => match control {
                    Some(Control::Restart) => {
                        break ChildLoopExit::Restart;
                    }
                    Some(Control::Reconfigure(next_config)) => {
                        config = next_config;
                        break ChildLoopExit::Restart;
                    }
                    None => {
                        break ChildLoopExit::Stop;
                    }
                },
                outbound = outbound_rx.recv() => match outbound {
                    Some(command) => {
                        if let Err(error) = write_panel_command(&mut stdin, &command).await {
                            tracing::warn!(%error, applet = %name, "exec applet failed to write to child");
                            break ChildLoopExit::ProtocolEnded;
                        }
                    }
                    None => {
                        break ChildLoopExit::Stop;
                    }
                },
                line = stdout_lines.next_line() => match line {
                    Ok(Some(BoundedLine::Line(line))) => match parse_child_line(&line) {
                        Ok(command) => send_child_command(&out, command),
                        Err(error) => tracing::debug!(%error, raw = %line, applet = %name, "exec applet ignored child line"),
                    },
                    Ok(Some(BoundedLine::Oversize(bytes))) => {
                        tracing::warn!(applet = %name, bytes, max = MAX_LINE_BYTES, "dropped oversize exec applet stdout line");
                    }
                    Ok(None) => break ChildLoopExit::ProtocolEnded,
                    Err(error) => {
                        tracing::warn!(%error, applet = %name, "exec applet stdout read failed");
                        break ChildLoopExit::ProtocolEnded;
                    }
                },
                line = stderr_lines.next_line(), if stderr_open => match line {
                    Ok(Some(BoundedLine::Line(line))) => {
                        if !line.is_empty() {
                            stderr_limiter.log(&name, &line);
                        }
                    }
                    Ok(Some(BoundedLine::Oversize(bytes))) => {
                        tracing::warn!(applet = %name, bytes, max = MAX_LINE_BYTES, "dropped oversize exec applet stderr line");
                    }
                    Ok(None) => {
                        stderr_limiter.flush(&name);
                        stderr_open = false;
                    }
                    Err(error) => {
                        stderr_limiter.flush(&name);
                        stderr_open = false;
                        tracing::warn!(%error, applet = %name, "exec applet stderr read failed");
                    }
                },
            }
        };

        stderr_limiter.flush(&name);
        finish_child(&mut child, &name).await;

        let _ = out.send(Input::ChildExited);
        if matches!(exit, ChildLoopExit::Stop) {
            return;
        }
        if matches!(exit, ChildLoopExit::ProtocolEnded) {
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
        }
    }
}

struct StderrLogLimiter {
    window_started: Instant,
    emitted: usize,
    suppressed: usize,
}

impl Default for StderrLogLimiter {
    fn default() -> Self {
        Self {
            window_started: Instant::now(),
            emitted: 0,
            suppressed: 0,
        }
    }
}

impl StderrLogLimiter {
    fn log(&mut self, applet: &str, line: &str) {
        if self.window_started.elapsed() >= STDERR_LOG_WINDOW {
            self.flush(applet);
            self.window_started = Instant::now();
            self.emitted = 0;
        }

        if self.emitted < STDERR_LOG_LIMIT {
            self.emitted += 1;
            tracing::warn!(stderr = %line, applet = %applet, "exec applet child stderr");
        } else {
            self.suppressed += 1;
        }
    }

    fn flush(&mut self, applet: &str) {
        if self.suppressed > 0 {
            tracing::warn!(
                applet = %applet,
                suppressed = self.suppressed,
                "exec applet child stderr lines suppressed"
            );
            self.suppressed = 0;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildLoopExit {
    Restart,
    Stop,
    ProtocolEnded,
}

async fn finish_child(child: &mut tokio::process::Child, name: &str) {
    match child.try_wait() {
        Ok(Some(status)) => {
            tracing::info!(?status, applet = %name, "exec applet child exited");
        }
        Ok(None) => {
            tracing::debug!(applet = %name, "exec applet child protocol ended before process exit; terminating child");
            if let Err(error) = child.kill().await {
                tracing::warn!(%error, applet = %name, "exec applet failed to kill child");
            }
        }
        Err(error) => {
            tracing::warn!(%error, applet = %name, "exec applet child status check failed");
        }
    }
}

fn send_child_command(out: &relm4::Sender<Input>, command: ChildCommand) {
    let _ = out.send(match command {
        ChildCommand::Status(payload) => Input::StatusChanged(payload),
        ChildCommand::Popover(payload) => Input::PopoverChanged(payload),
    });
}

pub async fn write_panel_command(
    stdin: &mut tokio::process::ChildStdin,
    command: &PanelCommand,
) -> Result<(), std::io::Error> {
    let mut line = super::protocol::encode_panel_command(command).into_bytes();
    line.push(b'\n');
    stdin.write_all(&line).await?;
    stdin.flush().await
}

#[derive(Debug, PartialEq, Eq)]
enum BoundedLine {
    Line(String),
    Oversize(usize),
}

struct BoundedLines<R> {
    reader: R,
    buf: Vec<u8>,
    overflowed: bool,
    total: usize,
    max_bytes: usize,
}

impl<R: AsyncBufRead + Unpin> BoundedLines<R> {
    fn new(reader: R, max_bytes: usize) -> Self {
        Self {
            reader,
            buf: Vec::new(),
            overflowed: false,
            total: 0,
            max_bytes,
        }
    }

    async fn next_line(&mut self) -> std::io::Result<Option<BoundedLine>> {
        loop {
            let (consumed, has_newline, eof) = {
                let available = self.reader.fill_buf().await?;
                if available.is_empty() {
                    (0, false, true)
                } else {
                    let pos = available.iter().position(|&b| b == b'\n');
                    let to_consume = pos.map(|p| p + 1).unwrap_or(available.len());
                    if !self.overflowed {
                        let space = self.max_bytes.saturating_sub(self.buf.len());
                        let copy_len = to_consume.min(space);
                        self.buf.extend_from_slice(&available[..copy_len]);
                        if to_consume > space {
                            self.overflowed = true;
                        }
                    }
                    (to_consume, pos.is_some(), false)
                }
            };

            if eof {
                if self.total == 0 && !self.overflowed && self.buf.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(self.take_result()));
            }

            self.total += consumed;
            self.reader.consume(consumed);

            if has_newline {
                return Ok(Some(self.take_result()));
            }
        }
    }

    fn take_result(&mut self) -> BoundedLine {
        let total = std::mem::take(&mut self.total);
        let overflowed = std::mem::replace(&mut self.overflowed, false);
        let mut buf = std::mem::take(&mut self.buf);
        if overflowed {
            return BoundedLine::Oversize(total.max(buf.len()));
        }
        if buf.last() == Some(&b'\n') {
            buf.pop();
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
        }
        BoundedLine::Line(String::from_utf8_lossy(&buf).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::{Command as StdCommand, Stdio as StdStdio},
        time::Duration,
    };

    use super::*;
    use crate::applets::exec::{
        applet::Config,
        protocol::{StatusItem, StatusPayload},
    };

    #[tokio::test]
    async fn supervisor_delivers_fast_child_output_before_exit() {
        for _ in 0..25 {
            let (sender, receiver) = relm4::channel();
            let (_outbound_tx, outbound_rx) = mpsc::channel(1);
            let (_control_tx, control_rx) = mpsc::unbounded_channel();
            let config = Config {
                command: vec![
                    "/bin/sh".into(),
                    "-c".into(),
                    r#"printf 'diagnostic\n' >&2; printf 'status {"items":[{"id":"fast","label":"ok"}]}\n'"#.into(),
                ],
                restart_delay_ms: 60_000,
                options: serde_json::json!({}),
                env_clear: false,
                env: std::collections::HashMap::new(),
            };

            let task = tokio::spawn(run("fast".into(), config, outbound_rx, control_rx, sender));

            let first = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
                .await
                .expect("supervisor should emit first message")
                .expect("supervisor sender should stay alive");
            task.abort();

            assert!(matches!(
                first,
                Input::StatusChanged(StatusPayload {
                    items
                }) if items == vec![StatusItem {
                    id: Some("fast".into()),
                    icon: None,
                    label: Some("ok".into()),
                    tooltip: None,
                    menu: Vec::new(),
                }]
            ));
        }
    }

    #[tokio::test]
    async fn supervisor_reaps_child_that_closes_stdout_without_exiting() {
        let pid_path =
            std::env::temp_dir().join(format!("glimpse-exec-child-{}.pid", std::process::id()));
        let (sender, receiver) = relm4::channel();
        let (_outbound_tx, outbound_rx) = mpsc::channel(1);
        let (_control_tx, control_rx) = mpsc::unbounded_channel();
        let config = Config {
            command: vec![
                "/bin/sh".into(),
                "-c".into(),
                format!("echo $$ > {}; exec 1>&-; sleep 30", pid_path.display()),
            ],
            restart_delay_ms: 60_000,
            options: serde_json::json!({}),
            env_clear: false,
            env: std::collections::HashMap::new(),
        };

        let task = tokio::spawn(run("leaky".into(), config, outbound_rx, control_rx, sender));

        let _ = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("supervisor should emit child exit");
        let pid = fs::read_to_string(&pid_path)
            .expect("child should write pid")
            .trim()
            .to_string();
        let alive = process_alive(&pid);
        if alive {
            let _ = StdCommand::new("kill").arg("-TERM").arg(&pid).status();
        }
        let _ = fs::remove_file(pid_path);
        task.abort();

        assert!(!alive, "child process {pid} was left running");
    }

    fn process_alive(pid: &str) -> bool {
        StdCommand::new("kill")
            .arg("-0")
            .arg(pid)
            .stderr(StdStdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}
