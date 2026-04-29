use std::{collections::HashMap, env};

use anyhow::{Context, bail};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::mpsc,
};

use crate::compositors::compositors::CompositorEvent;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Niri;

impl Niri {
    pub async fn listen(self, sender: mpsc::Sender<CompositorEvent>) -> anyhow::Result<()> {
        let mut stream = connect().await?;
        write_request(&mut stream, &json!("EventStream")).await?;

        let reader = BufReader::new(stream);
        let mut lines = reader.lines();
        let reply = lines
            .next_line()
            .await?
            .context("niri event stream closed before initial reply")?;
        ensure_ok_reply(&reply)?;

        let mut state = NiriEventState::default();
        while let Some(line) = lines.next_line().await? {
            for event in parse_niri_event(&line, &mut state) {
                if sender.send(event).await.is_err() {
                    return Ok(());
                }
            }
        }

        Ok(())
    }

    pub async fn set_keyboard_layout(&self, layout: usize) -> anyhow::Result<()> {
        let layout = u8::try_from(layout).context("niri keyboard layout index is out of range")?;
        send_action(json!({
            "SwitchLayout": {
                "layout": {
                    "Index": layout
                }
            }
        }))
        .await
    }

    pub async fn set_workspace(&self, workspace: usize) -> anyhow::Result<()> {
        let workspace = u8::try_from(workspace).context("niri workspace index is out of range")?;
        send_action(json!({
            "FocusWorkspace": {
                "reference": {
                    "Index": workspace
                }
            }
        }))
        .await
    }

    pub async fn focus_next_workspace(&self) -> anyhow::Result<()> {
        send_action(json!({ "FocusWorkspaceDown": {} })).await
    }

    pub async fn focus_previous_workspace(&self) -> anyhow::Result<()> {
        send_action(json!({ "FocusWorkspaceUp": {} })).await
    }

    pub async fn focus_window(&self, window: usize) -> anyhow::Result<()> {
        send_action(json!({
            "FocusWindow": {
                "id": window as u64
            }
        }))
        .await
    }

    pub async fn focus_next_window(&self) -> anyhow::Result<()> {
        send_action(json!({ "FocusWindowDownOrColumnRight": {} })).await
    }

    pub async fn focus_previous_window(&self) -> anyhow::Result<()> {
        send_action(json!({ "FocusWindowUpOrColumnLeft": {} })).await
    }
}

async fn send_action(action: Value) -> anyhow::Result<()> {
    let mut stream = connect().await?;
    write_request(&mut stream, &json!({ "Action": action })).await?;

    let mut lines = BufReader::new(stream).lines();
    let reply = lines
        .next_line()
        .await?
        .context("niri action closed before reply")?;
    ensure_ok_reply(&reply)
}

async fn connect() -> anyhow::Result<UnixStream> {
    let socket = env::var("NIRI_SOCKET").context("NIRI_SOCKET is not set")?;
    UnixStream::connect(socket)
        .await
        .context("failed to connect to niri socket")
}

async fn write_request(stream: &mut UnixStream, request: &Value) -> anyhow::Result<()> {
    let mut bytes = serde_json::to_vec(request)?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

fn ensure_ok_reply(line: &str) -> anyhow::Result<()> {
    let reply: Value = serde_json::from_str(line).context("invalid niri reply")?;
    if let Some(error) = reply.get("Err").and_then(Value::as_str) {
        bail!("niri IPC error: {error}");
    }

    if reply.get("Ok").is_none() {
        bail!("unexpected niri IPC reply: {line}");
    }

    Ok(())
}

#[derive(Default)]
struct NiriEventState {
    current_workspace: Option<usize>,
    layout_names: Vec<String>,
    window_workspaces: HashMap<usize, usize>,
}

fn parse_niri_event(line: &str, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    let Ok(event) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };

    if let Some(workspaces) = event
        .get("WorkspacesChanged")
        .and_then(|event| event.get("workspaces"))
        .and_then(Value::as_array)
    {
        return parse_workspaces_changed(workspaces, state);
    }

    if let Some(event) = event.get("WorkspaceActivated") {
        return parse_workspace_activated(event, state);
    }

    if let Some(event) = event.get("WorkspaceActiveWindowChanged") {
        return parse_workspace_active_window_changed(event, state);
    }

    if let Some(windows) = event
        .get("WindowsChanged")
        .and_then(|event| event.get("windows"))
        .and_then(Value::as_array)
    {
        return parse_windows_changed(windows, state);
    }

    if let Some(window) = event
        .get("WindowOpenedOrChanged")
        .and_then(|event| event.get("window"))
    {
        return parse_window_changed(window, state);
    }

    if let Some(event) = event.get("WindowFocusChanged") {
        return parse_window_focus_changed(event, state);
    }

    if let Some(event) = event.get("KeyboardLayoutsChanged") {
        return parse_keyboard_layouts_changed(event, state);
    }

    if let Some(event) = event.get("KeyboardLayoutSwitched") {
        return parse_keyboard_layout_switched(event, state);
    }

    Vec::new()
}

fn parse_workspaces_changed(
    workspaces: &[Value],
    state: &mut NiriEventState,
) -> Vec<CompositorEvent> {
    let mut events = Vec::new();
    for workspace in workspaces {
        let Some(id) = field_usize(workspace, "id") else {
            continue;
        };

        if workspace
            .get("is_focused")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            state.current_workspace = Some(id);
            events.push(CompositorEvent::WorkspaceChanged(id));

            if let Some(window) = field_usize(workspace, "active_window_id") {
                events.push(CompositorEvent::FocusedWindowChanged {
                    workspace: id,
                    window,
                });
            }
        }
    }

    events
}

fn parse_workspace_activated(event: &Value, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    let focused = event
        .get("focused")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(workspace) = field_usize(event, "id") else {
        return Vec::new();
    };

    if focused {
        state.current_workspace = Some(workspace);
        vec![CompositorEvent::WorkspaceChanged(workspace)]
    } else {
        Vec::new()
    }
}

fn parse_workspace_active_window_changed(
    event: &Value,
    state: &mut NiriEventState,
) -> Vec<CompositorEvent> {
    let Some(workspace) = field_usize(event, "workspace_id") else {
        return Vec::new();
    };

    if state.current_workspace == Some(workspace)
        && let Some(window) = field_usize(event, "active_window_id")
    {
        return vec![CompositorEvent::FocusedWindowChanged { workspace, window }];
    }

    Vec::new()
}

fn parse_windows_changed(windows: &[Value], state: &mut NiriEventState) -> Vec<CompositorEvent> {
    state.window_workspaces.clear();
    let mut events = Vec::new();

    for window in windows {
        if let (Some(window_id), Some(workspace)) = (
            field_usize(window, "id"),
            field_usize(window, "workspace_id"),
        ) {
            state.window_workspaces.insert(window_id, workspace);

            if window
                .get("is_focused")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                events.push(CompositorEvent::FocusedWindowChanged {
                    workspace,
                    window: window_id,
                });
            }
        }
    }

    events
}

fn parse_window_changed(window: &Value, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    if let (Some(window_id), Some(workspace)) = (
        field_usize(window, "id"),
        field_usize(window, "workspace_id"),
    ) {
        state.window_workspaces.insert(window_id, workspace);

        if window
            .get("is_focused")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return vec![CompositorEvent::FocusedWindowChanged {
                workspace,
                window: window_id,
            }];
        }
    }

    Vec::new()
}

fn parse_window_focus_changed(event: &Value, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    let Some(window) = field_usize(event, "id") else {
        return Vec::new();
    };
    let Some(workspace) = state
        .window_workspaces
        .get(&window)
        .copied()
        .or(state.current_workspace)
    else {
        return Vec::new();
    };

    vec![CompositorEvent::FocusedWindowChanged { workspace, window }]
}

fn parse_keyboard_layouts_changed(
    event: &Value,
    state: &mut NiriEventState,
) -> Vec<CompositorEvent> {
    let Some(layouts) = event.get("keyboard_layouts") else {
        return Vec::new();
    };

    state.layout_names = layouts
        .get("names")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();

    let Some(index) = field_usize(layouts, "current_idx") else {
        return Vec::new();
    };

    vec![CompositorEvent::KeyboardLayoutChanged(layout_label(
        state, index,
    ))]
}

fn parse_keyboard_layout_switched(
    event: &Value,
    state: &mut NiriEventState,
) -> Vec<CompositorEvent> {
    let Some(index) = field_usize(event, "idx") else {
        return Vec::new();
    };

    vec![CompositorEvent::KeyboardLayoutChanged(layout_label(
        state, index,
    ))]
}

fn layout_label(state: &NiriEventState, index: usize) -> String {
    state
        .layout_names
        .get(index)
        .cloned()
        .unwrap_or_else(|| index.to_string())
}

fn field_usize(value: &Value, field: &str) -> Option<usize> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_workspace_and_focused_window_events() {
        let mut state = NiriEventState::default();

        let events = parse_niri_event(
            r#"{"WorkspacesChanged":{"workspaces":[{"id":4,"is_focused":true,"active_window_id":9}]}}"#,
            &mut state,
        );

        assert_eq!(
            events,
            vec![
                CompositorEvent::WorkspaceChanged(4),
                CompositorEvent::FocusedWindowChanged {
                    workspace: 4,
                    window: 9,
                }
            ]
        );
    }

    #[test]
    fn tracks_window_workspace_for_focus_events() {
        let mut state = NiriEventState::default();

        parse_niri_event(
            r#"{"WindowOpenedOrChanged":{"window":{"id":12,"workspace_id":6,"is_focused":false}}}"#,
            &mut state,
        );
        let events = parse_niri_event(r#"{"WindowFocusChanged":{"id":12}}"#, &mut state);

        assert_eq!(
            events,
            vec![CompositorEvent::FocusedWindowChanged {
                workspace: 6,
                window: 12,
            }]
        );
    }

    #[test]
    fn renders_keyboard_layout_name_when_known() {
        let mut state = NiriEventState::default();

        let events = parse_niri_event(
            r#"{"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":["us","de"],"current_idx":1}}}"#,
            &mut state,
        );

        assert_eq!(
            events,
            vec![CompositorEvent::KeyboardLayoutChanged("de".into())]
        );
    }
}
