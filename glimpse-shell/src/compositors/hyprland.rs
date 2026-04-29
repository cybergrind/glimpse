use std::{env, path::PathBuf};

use anyhow::{Context, bail, ensure};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::mpsc,
};

use crate::compositors::compositors::CompositorEvent;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hyprland;

impl Hyprland {
    pub async fn listen(self, sender: mpsc::Sender<CompositorEvent>) -> anyhow::Result<()> {
        let stream = UnixStream::connect(event_socket_path()?)
            .await
            .context("failed to connect to hyprland event socket")?;
        let reader = BufReader::new(stream);
        let mut lines = reader.lines();
        let mut state = HyprlandEventState::default();

        while let Some(line) = lines.next_line().await? {
            for event in parse_hyprland_event(&line, &mut state) {
                if sender.send(event).await.is_err() {
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    pub async fn set_keyboard_layout(&self, layout: usize) -> anyhow::Result<()> {
        send_command(format!("switchxkblayout all {layout}")).await
    }

    pub async fn set_workspace(&self, workspace: usize) -> anyhow::Result<()> {
        ensure!(
            workspace > 0 && workspace <= i32::MAX as usize,
            "hyprland workspace id must be between 1 and {}",
            i32::MAX
        );
        send_command(format!("dispatch workspace {workspace}")).await
    }

    pub async fn focus_next_workspace(&self) -> anyhow::Result<()> {
        send_command("dispatch workspace +1").await
    }

    pub async fn focus_previous_workspace(&self) -> anyhow::Result<()> {
        send_command("dispatch workspace -1").await
    }

    pub async fn focus_window(&self, window: usize) -> anyhow::Result<()> {
        send_command(format!("dispatch focuswindow address:0x{window:x}")).await
    }

    pub async fn focus_next_window(&self) -> anyhow::Result<()> {
        send_command("dispatch cyclenext").await
    }

    pub async fn focus_previous_window(&self) -> anyhow::Result<()> {
        send_command("dispatch cyclenext prev").await
    }
}

fn event_socket_path() -> anyhow::Result<PathBuf> {
    socket_path(".socket2.sock")
}

fn control_socket_path() -> anyhow::Result<PathBuf> {
    socket_path(".socket.sock")
}

fn socket_path(socket_name: &str) -> anyhow::Result<PathBuf> {
    let signature = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .context("HYPRLAND_INSTANCE_SIGNATURE is not set")?;
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());

    Ok(PathBuf::from(runtime_dir)
        .join("hypr")
        .join(signature)
        .join(socket_name))
}

async fn send_command(command: impl AsRef<str>) -> anyhow::Result<()> {
    let mut stream = UnixStream::connect(control_socket_path()?)
        .await
        .context("failed to connect to hyprland control socket")?;
    stream.write_all(command.as_ref().as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.shutdown().await?;

    let mut reply = String::new();
    stream.read_to_string(&mut reply).await?;
    let reply = reply.trim();

    if reply == "ok" || reply.is_empty() {
        Ok(())
    } else {
        bail!("hyprland IPC command failed: {reply}");
    }
}

#[derive(Default)]
struct HyprlandEventState {
    current_workspace: Option<usize>,
}

fn parse_hyprland_event(line: &str, state: &mut HyprlandEventState) -> Vec<CompositorEvent> {
    if let Some(payload) = line.strip_prefix("workspacev2>>") {
        if let Some(workspace) = payload
            .split(',')
            .next()
            .and_then(|workspace| workspace.parse::<usize>().ok())
        {
            state.current_workspace = Some(workspace);
            return vec![CompositorEvent::WorkspaceChanged(workspace)];
        }
    }

    if let Some(payload) = line.strip_prefix("workspace>>") {
        if let Ok(workspace) = payload.parse::<usize>() {
            state.current_workspace = Some(workspace);
            return vec![CompositorEvent::WorkspaceChanged(workspace)];
        }
    }

    if let Some(payload) = line.strip_prefix("focusedmonv2>>") {
        if let Some(workspace) = payload.split(',').nth(1).and_then(parse_usize) {
            state.current_workspace = Some(workspace);
            return vec![CompositorEvent::WorkspaceChanged(workspace)];
        }
    }

    if let Some(payload) = line.strip_prefix("activewindowv2>>") {
        if let (Some(workspace), Some(window)) = (
            state.current_workspace,
            parse_hyprland_window_address(payload),
        ) {
            return vec![CompositorEvent::FocusedWindowChanged { workspace, window }];
        }
    }

    if let Some(payload) = line.strip_prefix("activelayout>>") {
        if let Some(layout) = payload
            .split(',')
            .nth(1)
            .filter(|layout| !layout.is_empty())
        {
            return vec![CompositorEvent::KeyboardLayoutChanged(layout.to_owned())];
        }
    }

    Vec::new()
}

fn parse_hyprland_window_address(value: &str) -> Option<usize> {
    let value = value.trim();
    let value = value.strip_prefix("0x").unwrap_or(value);
    usize::from_str_radix(value, 16).ok()
}

fn parse_usize(value: &str) -> Option<usize> {
    value.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_workspace_events() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("workspacev2>>3,code", &mut state),
            vec![CompositorEvent::WorkspaceChanged(3)]
        );
        assert_eq!(
            parse_hyprland_event("focusedmonv2>>DP-1,4", &mut state),
            vec![CompositorEvent::WorkspaceChanged(4)]
        );
    }

    #[test]
    fn parses_focused_window_after_workspace_is_known() {
        let mut state = HyprlandEventState::default();
        parse_hyprland_event("workspacev2>>3,code", &mut state);

        assert_eq!(
            parse_hyprland_event("activewindowv2>>3f2", &mut state),
            vec![CompositorEvent::FocusedWindowChanged {
                workspace: 3,
                window: 0x3f2,
            }]
        );
    }

    #[test]
    fn parses_keyboard_layout_events() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("activelayout>>keyboard,English (US)", &mut state),
            vec![CompositorEvent::KeyboardLayoutChanged(
                "English (US)".into()
            )]
        );
    }
}
