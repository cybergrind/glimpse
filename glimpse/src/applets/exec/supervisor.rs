use std::{process::Stdio, time::Duration};

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};

use super::{
    ExecConfig,
    applet::ExecMsg,
    protocol::{ChildMessage, InitData, PanelMessage},
};

#[derive(Debug)]
pub enum SupervisorControl {
    Restart,
    Reconfigure(ExecConfig),
}

pub async fn run_supervisor(
    name: String,
    mut config: ExecConfig,
    mut outbound_rx: mpsc::UnboundedReceiver<PanelMessage>,
    mut restart_rx: mpsc::UnboundedReceiver<SupervisorControl>,
    out: relm4::Sender<ExecMsg>,
) {
    loop {
        let program = match config.command.first() {
            Some(program) => program.clone(),
            None => {
                tracing::error!(applet = %name, "exec applet: empty command");
                let _ = out.send(ExecMsg::ChildExited);
                return;
            }
        };

        tracing::info!(applet = %name, program = %program, "exec applet: spawning child");
        let mut child = match Command::new(&program)
            .args(config.command.iter().skip(1))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                tracing::warn!(%error, applet = %name, "exec applet: failed to spawn child");
                let _ = out.send(ExecMsg::ChildExited);
                tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
                continue;
            }
        };

        let Some(mut stdin) = child.stdin.take() else {
            tracing::warn!(applet = %name, "exec applet: child missing stdin");
            let _ = out.send(ExecMsg::ChildExited);
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
            continue;
        };
        let Some(stdout) = child.stdout.take() else {
            tracing::warn!(applet = %name, "exec applet: child missing stdout");
            let _ = out.send(ExecMsg::ChildExited);
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
            continue;
        };

        if let Err(error) = write_message(
            &mut stdin,
            &PanelMessage::Init(InitData {
                instance: name.clone(),
                options: config.options.clone(),
            }),
        )
        .await
        {
            tracing::warn!(%error, applet = %name, "exec applet: failed to send init");
        }

        let mut lines = BufReader::new(stdout).lines();
        let mut should_stop = false;
        let mut restart_now = false;
        loop {
            tokio::select! {
                maybe_restart = restart_rx.recv() => {
                    match maybe_restart {
                        Some(SupervisorControl::Restart) => {
                            tracing::info!(applet = %name, "exec applet: restart requested");
                            restart_now = true;
                            let _ = child.kill().await;
                            break;
                        }
                        Some(SupervisorControl::Reconfigure(next_config)) => {
                            tracing::info!(applet = %name, "exec applet: config updated");
                            config = next_config;
                            restart_now = true;
                            let _ = child.kill().await;
                            break;
                        }
                        None => {
                            should_stop = true;
                            let _ = child.kill().await;
                            break;
                        }
                    }
                }
                maybe_message = outbound_rx.recv() => {
                    match maybe_message {
                        Some(message) => {
                            if let Err(error) = write_message(&mut stdin, &message).await {
                                tracing::warn!(%error, applet = %name, "exec applet: failed to write to child");
                                break;
                            }
                        }
                        None => {
                            should_stop = true;
                            let _ = child.kill().await;
                            break;
                        }
                    }
                }
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) => match serde_json::from_str::<ChildMessage>(&line) {
                            Ok(message) => {
                                let _ = out.send(ExecMsg::ChildMessage(message));
                            }
                            Err(error) => {
                                tracing::debug!(%error, raw = %line, applet = %name, "exec applet: invalid child message");
                            }
                        },
                        Ok(None) => break,
                        Err(error) => {
                            tracing::warn!(%error, applet = %name, "exec applet: stdout read failed");
                            break;
                        }
                    }
                }
                status = child.wait() => {
                    match status {
                        Ok(status) => tracing::info!(?status, applet = %name, "exec applet: child exited"),
                        Err(error) => tracing::warn!(%error, applet = %name, "exec applet: child wait failed"),
                    }
                    break;
                }
            }
        }

        let _ = out.send(ExecMsg::ChildExited);
        if should_stop {
            return;
        }
        if !restart_now {
            tokio::time::sleep(Duration::from_millis(config.restart_delay_ms)).await;
        }
    }
}

pub async fn write_message(
    stdin: &mut tokio::process::ChildStdin,
    message: &PanelMessage,
) -> Result<(), std::io::Error> {
    let encoded = encode_message_line(message);
    stdin.write_all(&encoded).await?;
    stdin.flush().await
}

pub fn encode_message_line(message: &PanelMessage) -> Vec<u8> {
    let mut encoded =
        serde_json::to_vec(message).expect("exec panel messages should always serialize");
    encoded.push(b'\n');
    encoded
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use relm4::channel;
    use serde_json::Value;
    use tokio::time::timeout;

    use super::{SupervisorControl, encode_message_line, run_supervisor};
    use crate::applets::exec::{
        ExecConfig,
        applet::ExecMsg,
        protocol::{ChildMessage, InitData, PanelMessage, StatusData},
    };

    #[test]
    fn init_messages_are_encoded_as_json_lines() {
        let encoded = encode_message_line(&PanelMessage::Init(InitData {
            instance: "exec-demo".into(),
            options: serde_json::json!({"theme": "dark"}),
        }));

        let text = String::from_utf8(encoded).expect("line should be utf-8");
        assert_eq!(
            text,
            "{\"type\":\"init\",\"data\":{\"instance\":\"exec-demo\",\"options\":{\"theme\":\"dark\"}}}\n"
        );
    }

    #[tokio::test]
    async fn supervisor_sends_init_to_real_subprocess() {
        let temp_dir = make_temp_dir("exec-init");
        let init_path = temp_dir.join("init.jsonl");
        let script_path = temp_dir.join("child.sh");
        write_script(
            &script_path,
            &format!(
                "#!/usr/bin/env bash\nread line\nprintf '%s\\n' \"$line\" > {}\nprintf '%s\\n' '{{\"type\":\"status\",\"data\":{{\"items\":[{{\"id\":\"demo\",\"text\":\"ready\"}}]}}}}'\nexit 0\n",
                shell_single_quote(&init_path)
            ),
        );

        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_restart_tx, restart_rx) = tokio::sync::mpsc::unbounded_channel();
        let (sender, receiver) = channel();
        let task = tokio::spawn(run_supervisor(
            "demo".into(),
            ExecConfig {
                command: vec![script_path.to_string_lossy().into_owned()],
                restart_delay_ms: 10_000,
                options: serde_json::json!({"env": "test"}),
            },
            outbound_rx,
            restart_rx,
            sender,
        ));

        let message = recv_exec_msg(&receiver).await;
        assert!(matches!(
            message,
            ExecMsg::ChildMessage(ChildMessage::Status(StatusData { .. }))
        ));

        let init_line = fs::read_to_string(&init_path).expect("child should capture init");
        let init: Value = serde_json::from_str(init_line.trim()).expect("init should be json");
        assert_eq!(init["type"], "init");
        assert_eq!(init["data"]["instance"], "demo");
        assert_eq!(init["data"]["options"]["env"], "test");

        drop(outbound_tx);
        task.abort();
        let _ = task.await;
    }

    #[tokio::test]
    async fn supervisor_restarts_real_subprocess_after_exit() {
        let temp_dir = make_temp_dir("exec-restart");
        let counter_path = temp_dir.join("counter");
        let script_path = temp_dir.join("child.sh");
        write_script(
            &script_path,
            &format!(
                "#!/usr/bin/env bash\ncount=0\nif [ -f {counter} ]; then count=$(cat {counter}); fi\ncount=$((count + 1))\nprintf '%s' \"$count\" > {counter}\nprintf '%s\\n' \"{{\\\"type\\\":\\\"status\\\",\\\"data\\\":{{\\\"items\\\":[{{\\\"id\\\":\\\"demo\\\",\\\"text\\\":\\\"run$count\\\"}}]}}}}\"\nexit 0\n",
                counter = shell_single_quote(&counter_path)
            ),
        );

        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        let (_restart_tx, restart_rx) = tokio::sync::mpsc::unbounded_channel();
        let (sender, receiver) = channel();
        let task = tokio::spawn(run_supervisor(
            "demo".into(),
            ExecConfig {
                command: vec![script_path.to_string_lossy().into_owned()],
                restart_delay_ms: 50,
                options: Default::default(),
            },
            outbound_rx,
            restart_rx,
            sender,
        ));

        let first = recv_exec_msg(&receiver).await;
        assert_status_text(&first, "run1");

        let second = recv_exec_msg(&receiver).await;
        assert!(matches!(second, ExecMsg::ChildExited));

        let third = recv_exec_msg(&receiver).await;
        assert_status_text(&third, "run2");

        drop(outbound_tx);
        task.abort();
        let _ = task.await;
    }

    #[tokio::test]
    async fn supervisor_restarts_immediately_when_requested() {
        let temp_dir = make_temp_dir("exec-restart-now");
        let counter_path = temp_dir.join("counter");
        let script_path = temp_dir.join("child.sh");
        write_script(
            &script_path,
            &format!(
                "#!/usr/bin/env bash\ncount=0\nif [ -f {counter} ]; then count=$(cat {counter}); fi\ncount=$((count + 1))\nprintf '%s' \"$count\" > {counter}\nprintf '%s\\n' \"{{\\\"type\\\":\\\"status\\\",\\\"data\\\":{{\\\"items\\\":[{{\\\"id\\\":\\\"demo\\\",\\\"text\\\":\\\"run$count\\\"}}]}}}}\"\nsleep 30\n",
                counter = shell_single_quote(&counter_path)
            ),
        );

        let (outbound_tx, outbound_rx) = tokio::sync::mpsc::unbounded_channel();
        let (restart_tx, restart_rx) = tokio::sync::mpsc::unbounded_channel();
        let (sender, receiver) = channel();
        let task = tokio::spawn(run_supervisor(
            "demo".into(),
            ExecConfig {
                command: vec![script_path.to_string_lossy().into_owned()],
                restart_delay_ms: 60_000,
                options: Default::default(),
            },
            outbound_rx,
            restart_rx,
            sender,
        ));

        let first = recv_exec_msg(&receiver).await;
        assert_status_text(&first, "run1");

        restart_tx
            .send(SupervisorControl::Restart)
            .expect("restart request should be queued");

        let second = recv_exec_msg(&receiver).await;
        assert!(matches!(second, ExecMsg::ChildExited));

        let third = recv_exec_msg(&receiver).await;
        assert_status_text(&third, "run2");

        drop(outbound_tx);
        drop(restart_tx);
        task.abort();
        let _ = task.await;
    }

    async fn recv_exec_msg(receiver: &relm4::Receiver<ExecMsg>) -> ExecMsg {
        timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("supervisor should emit a message within timeout")
            .expect("receiver should stay open")
    }

    fn assert_status_text(message: &ExecMsg, expected: &str) {
        let ExecMsg::ChildMessage(ChildMessage::Status(StatusData { items })) = message else {
            panic!("expected status message, got {message:?}");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text.as_deref(), Some(expected));
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("glimpse-{prefix}-{}-{unique}", nanos));
        fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn write_script(path: &Path, script: &str) {
        fs::write(path, script).expect("script should be written");
        let mut permissions = fs::metadata(path)
            .expect("script metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("script should be executable");
    }

    fn shell_single_quote(path: &Path) -> String {
        let raw = path.to_string_lossy();
        format!("'{}'", raw.replace('\'', "'\"'\"'"))
    }
}
