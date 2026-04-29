use std::{env, path::PathBuf};

use anyhow::{Context, bail, ensure};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::mpsc,
};

use crate::compositors::compositors::{
    CompositorCapabilities, CompositorEvent, CompositorRefresh, CompositorSnapshot,
    CompositorStructureSnapshot, KeyboardLayout, KeyboardLayoutSnapshot, Monitor, Window,
    Workspace,
};

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

    pub async fn snapshot(&self) -> anyhow::Result<CompositorSnapshot> {
        let structure = self.structure_snapshot().await?;
        let keyboard = self.keyboard_layout_snapshot().await?;

        Ok(CompositorSnapshot {
            capabilities: self.capabilities(),
            windows: structure.windows,
            workspaces: structure.workspaces,
            monitors: structure.monitors,
            keyboard_layouts: keyboard.keyboard_layouts,
            current_keyboard_layout: keyboard.current_keyboard_layout,
            focused_window: structure.focused_window,
            current_workspace: structure.current_workspace,
        })
    }

    pub async fn structure_snapshot(&self) -> anyhow::Result<CompositorStructureSnapshot> {
        let monitors = parse_monitors(&json_command("j/monitors").await?);
        let mut workspaces = parse_workspaces(&json_command("j/workspaces").await?);
        let mut windows = parse_windows(&json_command("j/clients").await?);
        let active_window = json_command("j/activewindow")
            .await
            .ok()
            .and_then(|value| parse_window_id(value.get("address")?));
        let current_workspace = monitors
            .iter()
            .find(|monitor| monitor.focused)
            .and_then(|monitor| monitor.active_workspace)
            .or_else(|| {
                workspaces
                    .iter()
                    .find(|workspace| workspace.focused)
                    .map(|workspace| workspace.id)
            });
        for window in &mut windows {
            window.focused = Some(window.id) == active_window;
        }
        for workspace in &mut workspaces {
            workspace.active = monitors
                .iter()
                .any(|monitor| monitor.active_workspace == Some(workspace.id));
            workspace.focused = current_workspace == Some(workspace.id);
        }

        Ok(CompositorStructureSnapshot {
            windows,
            workspaces,
            monitors,
            focused_window: active_window,
            current_workspace,
        })
    }

    pub async fn keyboard_layout_snapshot(&self) -> anyhow::Result<KeyboardLayoutSnapshot> {
        let (keyboard_layouts, current_keyboard_layout) = read_keyboard_layouts().await;

        Ok(KeyboardLayoutSnapshot {
            keyboard_layouts,
            current_keyboard_layout,
        })
    }

    pub fn capabilities(&self) -> CompositorCapabilities {
        CompositorCapabilities {
            windows: true,
            workspaces: true,
            monitors: true,
            keyboard_layouts: true,
            focused_window: true,
            current_workspace: true,
            fullscreen: true,
            floating: true,
            window_titles: true,
            night_light: false,
        }
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
    let reply = control_command(command).await?;
    let reply = reply.trim();

    if reply == "ok" || reply.is_empty() {
        Ok(())
    } else {
        bail!("hyprland IPC command failed: {reply}");
    }
}

async fn json_command(command: impl AsRef<str>) -> anyhow::Result<Value> {
    let reply = control_command(command).await?;
    serde_json::from_str(reply.trim()).context("invalid hyprland JSON reply")
}

async fn control_command(command: impl AsRef<str>) -> anyhow::Result<String> {
    let mut stream = UnixStream::connect(control_socket_path()?)
        .await
        .context("failed to connect to hyprland control socket")?;
    stream.write_all(command.as_ref().as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.shutdown().await?;

    let mut reply = String::new();
    stream.read_to_string(&mut reply).await?;
    Ok(reply)
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
            return vec![
                CompositorEvent::WorkspaceChanged {
                    id: workspace,
                    focused: true,
                },
                CompositorEvent::RefreshRequested(CompositorRefresh::STRUCTURE),
            ];
        }
    }

    if let Some(payload) = line.strip_prefix("workspace>>") {
        if let Ok(workspace) = payload.parse::<usize>() {
            state.current_workspace = Some(workspace);
            return vec![
                CompositorEvent::WorkspaceChanged {
                    id: workspace,
                    focused: true,
                },
                CompositorEvent::RefreshRequested(CompositorRefresh::STRUCTURE),
            ];
        }
    }

    if let Some(payload) = line.strip_prefix("focusedmonv2>>") {
        let mut parts = payload.split(',');
        let monitor = parts.next().filter(|monitor| !monitor.is_empty());
        let workspace = parts.next().and_then(parse_usize);
        if let Some(workspace) = workspace {
            state.current_workspace = Some(workspace);
            let mut events = Vec::new();
            if let Some(monitor) = monitor {
                events.push(CompositorEvent::MonitorChanged {
                    name: monitor.to_owned(),
                    active_workspace: Some(workspace),
                    focused: true,
                });
            }
            events.push(CompositorEvent::WorkspaceChanged {
                id: workspace,
                focused: true,
            });
            return events;
        }
    }

    if let Some(payload) = line.strip_prefix("activewindowv2>>") {
        return vec![CompositorEvent::FocusedWindowChanged(
            parse_hyprland_window_address(payload),
        )];
    }

    if let Some(payload) = line.strip_prefix("fullscreen>>") {
        if let Some(fullscreen) = parse_bool_int(payload) {
            return vec![CompositorEvent::WindowFullscreenChanged {
                window: None,
                fullscreen,
            }];
        }
    }

    if let Some(payload) = line.strip_prefix("changefloatingmode>>") {
        let mut parts = payload.split(',');
        if let (Some(window), Some(floating)) = (
            parts.next().and_then(parse_hyprland_window_address),
            parts.next().and_then(parse_bool_int),
        ) {
            return vec![CompositorEvent::WindowFloatingChanged { window, floating }];
        }
    }

    if let Some(payload) = line.strip_prefix("windowtitlev2>>") {
        let mut parts = payload.splitn(2, ',');
        if let (Some(window), Some(title)) = (
            parts.next().and_then(parse_hyprland_window_address),
            parts.next().filter(|title| !title.is_empty()),
        ) {
            return vec![CompositorEvent::WindowTitleChanged {
                window,
                title: title.to_owned(),
            }];
        }
    }

    if let Some(payload) = line.strip_prefix("activelayout>>") {
        if let Some(layout) = payload
            .split(',')
            .nth(1)
            .filter(|layout| !layout.is_empty())
        {
            return vec![CompositorEvent::KeyboardLayoutChanged {
                index: None,
                name: Some(layout.to_owned()),
            }];
        }
    }

    if is_structural_event(line) {
        return vec![CompositorEvent::RefreshRequested(
            CompositorRefresh::STRUCTURE,
        )];
    }

    Vec::new()
}

fn is_structural_event(line: &str) -> bool {
    [
        "monitorremoved>>",
        "monitorremovedv2>>",
        "monitoradded>>",
        "monitoraddedv2>>",
        "createworkspace>>",
        "createworkspacev2>>",
        "destroyworkspace>>",
        "destroyworkspacev2>>",
        "moveworkspace>>",
        "moveworkspacev2>>",
        "renameworkspace>>",
        "openwindow>>",
        "closewindow>>",
        "movewindow>>",
        "movewindowv2>>",
        "windowtitle>>",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn parse_hyprland_window_address(value: &str) -> Option<usize> {
    let value = value.trim();
    let value = value.strip_prefix("0x").unwrap_or(value);
    usize::from_str_radix(value, 16).ok()
}

fn parse_usize(value: &str) -> Option<usize> {
    value.parse::<usize>().ok()
}

fn parse_bool_int(value: &str) -> Option<bool> {
    match value.trim() {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

fn parse_monitors(value: &Value) -> Vec<Monitor> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|monitor| {
            let name = field_string(monitor, "name")?;
            Some(Monitor {
                id: field_usize(monitor, "id"),
                name,
                description: field_string(monitor, "description"),
                active_workspace: monitor
                    .get("activeWorkspace")
                    .and_then(|workspace| field_usize(workspace, "id")),
                focused: monitor
                    .get("focused")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn parse_workspaces(value: &Value) -> Vec<Workspace> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|workspace| {
            Some(Workspace {
                id: field_usize(workspace, "id")?,
                index: field_usize(workspace, "id"),
                name: field_string(workspace, "name"),
                monitor: field_string(workspace, "monitor"),
                active: false,
                focused: false,
                urgent: false,
                active_window: field_usize(workspace, "lastwindow"),
            })
        })
        .collect()
}

fn parse_windows(value: &Value) -> Vec<Window> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(parse_window)
        .collect()
}

fn parse_window(value: &Value) -> Option<Window> {
    Some(Window {
        id: parse_window_id(value.get("address")?)?,
        title: field_string(value, "title"),
        app_id: field_string(value, "class"),
        pid: value
            .get("pid")
            .and_then(Value::as_i64)
            .and_then(|pid| i32::try_from(pid).ok()),
        layout_order: None,
        workspace: value
            .get("workspace")
            .and_then(|workspace| field_usize(workspace, "id")),
        focused: false,
        urgent: value
            .get("urgent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fullscreen: value
            .get("fullscreen")
            .and_then(Value::as_i64)
            .map(|fullscreen| fullscreen != 0)
            .unwrap_or(false),
        floating: value.get("floating").and_then(Value::as_bool),
    })
}

async fn read_keyboard_layouts() -> (Vec<KeyboardLayout>, Option<usize>) {
    let names = json_command("j/getoption input:kb_layout")
        .await
        .ok()
        .and_then(|value| field_string(&value, "str"))
        .map(|layouts| {
            layouts
                .split(',')
                .map(str::trim)
                .filter(|layout| !layout.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let active = active_keymap().await;
    let current = active
        .as_deref()
        .and_then(|active| names.iter().position(|name| name == active));

    (
        names
            .into_iter()
            .enumerate()
            .map(|(index, name)| KeyboardLayout { index, name })
            .collect(),
        current,
    )
}

async fn active_keymap() -> Option<String> {
    let devices = json_command("j/devices").await.ok()?;
    devices
        .get("keyboards")?
        .as_array()?
        .iter()
        .find(|keyboard| {
            keyboard
                .get("main")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .or_else(|| devices.get("keyboards")?.as_array()?.first())?
        .get("active_keymap")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn parse_window_id(value: &Value) -> Option<usize> {
    value.as_str().and_then(parse_hyprland_window_address)
}

fn field_string(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn field_usize(value: &Value, field: &str) -> Option<usize> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .and_then(|value| usize::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_workspace_events() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("workspacev2>>3,code", &mut state),
            vec![
                CompositorEvent::WorkspaceChanged {
                    id: 3,
                    focused: true,
                },
                CompositorEvent::RefreshRequested(CompositorRefresh::STRUCTURE),
            ]
        );
        assert_eq!(
            parse_hyprland_event("focusedmonv2>>DP-1,4", &mut state),
            vec![
                CompositorEvent::MonitorChanged {
                    name: "DP-1".into(),
                    active_workspace: Some(4),
                    focused: true,
                },
                CompositorEvent::WorkspaceChanged {
                    id: 4,
                    focused: true,
                },
            ]
        );
    }

    #[test]
    fn parses_focused_window_after_workspace_is_known() {
        let mut state = HyprlandEventState::default();
        parse_hyprland_event("workspacev2>>3,code", &mut state);

        assert_eq!(
            parse_hyprland_event("activewindowv2>>3f2", &mut state),
            vec![CompositorEvent::FocusedWindowChanged(Some(0x3f2))]
        );
    }

    #[test]
    fn parses_keyboard_layout_events() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("activelayout>>keyboard,English (US)", &mut state),
            vec![CompositorEvent::KeyboardLayoutChanged {
                index: None,
                name: Some("English (US)".into())
            }]
        );
    }

    #[test]
    fn structural_window_events_request_refresh() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("openwindow>>3f2,1,kitty,Terminal", &mut state),
            vec![CompositorEvent::RefreshRequested(
                CompositorRefresh::STRUCTURE
            )]
        );
    }

    #[test]
    fn parses_window_update_events() {
        let mut state = HyprlandEventState::default();

        assert_eq!(
            parse_hyprland_event("windowtitlev2>>3f2,Terminal", &mut state),
            vec![CompositorEvent::WindowTitleChanged {
                window: 0x3f2,
                title: "Terminal".into(),
            }]
        );
        assert_eq!(
            parse_hyprland_event("fullscreen>>1", &mut state),
            vec![CompositorEvent::WindowFullscreenChanged {
                window: None,
                fullscreen: true,
            }]
        );
        assert_eq!(
            parse_hyprland_event("changefloatingmode>>3f2,1", &mut state),
            vec![CompositorEvent::WindowFloatingChanged {
                window: 0x3f2,
                floating: true,
            }]
        );
    }
}
