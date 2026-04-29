use std::{collections::HashMap, env};

use anyhow::{Context, bail};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::mpsc,
};

use crate::compositors::compositors::{
    CompositorCapabilities, CompositorEvent, CompositorSnapshot, KeyboardLayout, Monitor, Window,
    Workspace,
};

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

    pub async fn snapshot(&self) -> anyhow::Result<CompositorSnapshot> {
        let mut monitors = request_ok(json!("Outputs"))
            .await?
            .get("Outputs")
            .map(parse_outputs)
            .unwrap_or_default();
        let workspaces: Vec<Workspace> = request_ok(json!("Workspaces"))
            .await?
            .get("Workspaces")
            .and_then(Value::as_array)
            .map(|workspaces| workspaces.iter().filter_map(parse_workspace).collect())
            .unwrap_or_default();
        let windows = request_ok(json!("Windows"))
            .await?
            .get("Windows")
            .and_then(Value::as_array)
            .map(|windows| windows.iter().filter_map(parse_window).collect::<Vec<_>>())
            .unwrap_or_default();
        let (keyboard_layouts, current_keyboard_layout) =
            parse_keyboard_layouts_response(&request_ok(json!("KeyboardLayouts")).await?);
        let focused_window = request_ok(json!("FocusedWindow"))
            .await?
            .get("FocusedWindow")
            .and_then(|window| {
                if window.is_null() {
                    None
                } else {
                    field_usize(window, "id")
                }
            });
        let focused_output = request_ok(json!("FocusedOutput"))
            .await
            .ok()
            .and_then(|reply| {
                reply
                    .get("FocusedOutput")
                    .and_then(|output| {
                        if output.is_null() {
                            None
                        } else {
                            output.get("name").and_then(Value::as_str)
                        }
                    })
                    .map(ToOwned::to_owned)
            });
        let current_workspace = workspaces
            .iter()
            .find(|workspace| workspace.focused)
            .map(|workspace| workspace.id);
        for monitor in &mut monitors {
            monitor.focused = focused_output.as_deref() == Some(monitor.name.as_str());
            monitor.active_workspace = workspaces
                .iter()
                .find(|workspace| {
                    workspace.active && workspace.monitor.as_deref() == Some(monitor.name.as_str())
                })
                .map(|workspace| workspace.id);
        }

        Ok(CompositorSnapshot {
            capabilities: self.capabilities(),
            windows,
            workspaces,
            monitors,
            keyboard_layouts,
            current_keyboard_layout,
            focused_window,
            current_workspace,
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
            floating: false,
            window_titles: true,
            night_light: false,
        }
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

async fn request_ok(request: Value) -> anyhow::Result<Value> {
    let mut stream = connect().await?;
    write_request(&mut stream, &request).await?;

    let mut lines = BufReader::new(stream).lines();
    let reply = lines
        .next_line()
        .await
        .context("failed to read niri reply")?
        .context("niri request closed before reply")?;
    let reply: Value = serde_json::from_str(&reply).context("invalid niri reply")?;
    if let Some(error) = reply.get("Err").and_then(Value::as_str) {
        bail!("niri IPC error: {error}");
    }

    reply
        .get("Ok")
        .cloned()
        .context("unexpected niri IPC reply without Ok")
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
    focused_window: Option<usize>,
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

    if let Some(event) = event.get("WindowClosed") {
        if let Some(window) = field_usize(event, "id") {
            state.window_workspaces.remove(&window);
            if state.focused_window == Some(window) {
                state.focused_window = None;
            }
            return vec![CompositorEvent::WindowClosed(window)];
        }
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
    let next = workspaces
        .iter()
        .filter_map(parse_workspace)
        .collect::<Vec<_>>();

    if let Some(workspace) = next.iter().find(|workspace| workspace.focused) {
        state.current_workspace = Some(workspace.id);
    }

    vec![CompositorEvent::WorkspacesChanged(next)]
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
        vec![CompositorEvent::WorkspaceChanged {
            id: workspace,
            focused,
        }]
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

    if state.current_workspace == Some(workspace) {
        return vec![CompositorEvent::WorkspaceActiveWindowChanged {
            workspace,
            window: field_usize(event, "active_window_id"),
        }];
    }

    Vec::new()
}

fn parse_windows_changed(windows: &[Value], state: &mut NiriEventState) -> Vec<CompositorEvent> {
    state.window_workspaces.clear();
    let next = windows
        .iter()
        .filter_map(parse_window)
        .inspect(|window| {
            if let Some(workspace) = window.workspace {
                state.window_workspaces.insert(window.id, workspace);
            }
        })
        .collect::<Vec<_>>();

    if let Some(window) = next.iter().find(|window| window.focused) {
        state.focused_window = Some(window.id);
        if let Some(workspace) = window.workspace {
            state.current_workspace = Some(workspace);
        }
    }

    vec![CompositorEvent::WindowsChanged(next)]
}

fn parse_window_changed(window: &Value, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    let Some(window) = parse_window(window) else {
        return Vec::new();
    };

    if let Some(workspace) = window.workspace {
        state.window_workspaces.insert(window.id, workspace);
    }

    if window.focused {
        state.focused_window = Some(window.id);
        if let Some(workspace) = window.workspace {
            state.current_workspace = Some(workspace);
        }
    }

    vec![CompositorEvent::WindowChanged(window)]
}

fn parse_window_focus_changed(event: &Value, state: &mut NiriEventState) -> Vec<CompositorEvent> {
    let window = field_usize(event, "id");
    state.focused_window = window;

    if let Some(workspace) = window.and_then(|window| state.window_workspaces.get(&window).copied())
    {
        state.current_workspace = Some(workspace);
    }

    vec![CompositorEvent::FocusedWindowChanged(window)]
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

    vec![CompositorEvent::KeyboardLayoutsChanged {
        layouts: keyboard_layouts(&state.layout_names),
        current: Some(index),
    }]
}

fn parse_keyboard_layout_switched(
    event: &Value,
    state: &mut NiriEventState,
) -> Vec<CompositorEvent> {
    let Some(index) = field_usize(event, "idx") else {
        return Vec::new();
    };

    vec![CompositorEvent::KeyboardLayoutChanged {
        index: Some(index),
        name: state.layout_names.get(index).cloned(),
    }]
}

fn parse_outputs(value: &Value) -> Vec<Monitor> {
    let Some(outputs) = value.as_object() else {
        return Vec::new();
    };

    outputs
        .iter()
        .map(|(name, output)| Monitor {
            id: None,
            name: output
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(name)
                .to_owned(),
            description: output
                .get("model")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            active_workspace: None,
            focused: false,
        })
        .collect()
}

fn parse_workspace(value: &Value) -> Option<Workspace> {
    Some(Workspace {
        id: field_usize(value, "id")?,
        name: value
            .get("name")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        monitor: value
            .get("output")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        active: value
            .get("is_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        focused: value
            .get("is_focused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        urgent: value
            .get("is_urgent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        active_window: field_usize(value, "active_window_id"),
    })
}

fn parse_window(value: &Value) -> Option<Window> {
    Some(Window {
        id: field_usize(value, "id")?,
        title: value
            .get("title")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        app_id: value
            .get("app_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        pid: value
            .get("pid")
            .and_then(Value::as_i64)
            .and_then(|pid| i32::try_from(pid).ok()),
        workspace: field_usize(value, "workspace_id"),
        focused: value
            .get("is_focused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        urgent: value
            .get("is_urgent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        fullscreen: value
            .get("is_fullscreen")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        floating: value.get("is_floating").and_then(Value::as_bool),
    })
}

fn parse_keyboard_layouts_response(value: &Value) -> (Vec<KeyboardLayout>, Option<usize>) {
    let Some(layouts) = value.get("KeyboardLayouts") else {
        return (Vec::new(), None);
    };
    let names = layouts
        .get("names")
        .and_then(Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    (
        keyboard_layouts(&names),
        field_usize(layouts, "current_idx"),
    )
}

fn keyboard_layouts(names: &[String]) -> Vec<KeyboardLayout> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| KeyboardLayout {
            index,
            name: name.clone(),
        })
        .collect()
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
            vec![CompositorEvent::WorkspacesChanged(vec![Workspace {
                id: 4,
                name: None,
                monitor: None,
                active: false,
                focused: true,
                urgent: false,
                active_window: Some(9),
            }])]
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
            vec![CompositorEvent::FocusedWindowChanged(Some(12))]
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
            vec![CompositorEvent::KeyboardLayoutsChanged {
                layouts: vec![
                    KeyboardLayout {
                        index: 0,
                        name: "us".into(),
                    },
                    KeyboardLayout {
                        index: 1,
                        name: "de".into(),
                    },
                ],
                current: Some(1),
            }]
        );
    }
}
