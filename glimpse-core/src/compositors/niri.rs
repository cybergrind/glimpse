use std::{collections::HashMap, env};

use anyhow::{Context, bail};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::mpsc,
};

use crate::compositors::compositors::{
    CompositorCapabilities, CompositorEvent, CompositorSnapshot, KeyboardLayout, Monitor,
    ScreencastControlCapability, ScreencastKind, ScreencastSession, ScreencastStateCapability,
    ScreencastTarget, Window, Workspace,
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

        let screencast_result = request_ok(json!("Casts")).await;
        let screencasts = screencast_result
            .as_ref()
            .ok()
            .and_then(|value| value.get("Casts"))
            .and_then(Value::as_array)
            .map(|casts| casts.iter().filter_map(parse_cast).collect())
            .unwrap_or_default();
        let mut capabilities = self.capabilities();
        if screencast_result.is_err() {
            capabilities.screencast_state = ScreencastStateCapability::None;
            capabilities.screencast_control = ScreencastControlCapability::None;
        }

        Ok(CompositorSnapshot {
            capabilities,
            windows,
            workspaces,
            monitors,
            screencasts,
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
            screencast_state: ScreencastStateCapability::Sessions,
            screencast_control: ScreencastControlCapability::StopSession,
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

    pub async fn stop_screencast(&self, session_id: &str) -> anyhow::Result<()> {
        let session_id = session_id
            .parse::<u64>()
            .context("niri screencast session id is not numeric")?;
        send_action(json!({
            "StopCast": {
                "session_id": session_id
            }
        }))
        .await
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

    if let Some(casts) = event
        .get("CastsChanged")
        .and_then(|event| event.get("casts"))
        .and_then(Value::as_array)
    {
        return vec![CompositorEvent::ScreencastsChanged(
            casts.iter().filter_map(parse_cast).collect(),
        )];
    }

    if let Some(cast) = event
        .get("CastStartedOrChanged")
        .and_then(|event| event.get("cast"))
        .and_then(parse_cast)
    {
        return vec![CompositorEvent::ScreencastChanged(cast)];
    }

    if let Some(stream_id) = event
        .get("CastStopped")
        .and_then(|event| event.get("stream_id"))
        .and_then(Value::as_u64)
    {
        return vec![CompositorEvent::ScreencastStopped(stream_id.to_string())];
    }

    Vec::new()
}

fn parse_cast(value: &Value) -> Option<ScreencastSession> {
    let stream_id = field_usize(value, "stream_id")?.to_string();
    let session_id = field_usize(value, "session_id").map(|id| id.to_string());
    let kind = parse_cast_kind(value.get("kind"));
    let target = parse_cast_target(value.get("target"));
    let active = value
        .get("is_active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let pipewire_node = value
        .get("pw_node_id")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok());
    let client_pid = value
        .get("pid")
        .and_then(Value::as_i64)
        .and_then(|pid| i32::try_from(pid).ok());

    Some(ScreencastSession {
        id: stream_id,
        session_id,
        kind,
        target,
        active,
        pipewire_node,
        client_pid,
        stoppable: kind == ScreencastKind::PipeWire,
    })
}

fn parse_cast_kind(value: Option<&Value>) -> ScreencastKind {
    let Some(value) = value else {
        return ScreencastKind::Unknown;
    };
    let text = tagged_value_name(value).to_ascii_lowercase();

    if text.contains("pipewire") {
        ScreencastKind::PipeWire
    } else if text.contains("wlr") || text.contains("screencopy") {
        ScreencastKind::WlrScreencopy
    } else {
        ScreencastKind::Unknown
    }
}

fn parse_cast_target(value: Option<&Value>) -> ScreencastTarget {
    let Some(value) = value else {
        return ScreencastTarget::Unknown;
    };
    let text = tagged_value_name(value).to_ascii_lowercase();

    if text.contains("output") || text.contains("monitor") {
        ScreencastTarget::Monitor
    } else if text.contains("window") {
        ScreencastTarget::Window
    } else {
        ScreencastTarget::Unknown
    }
}

fn tagged_value_name(value: &Value) -> String {
    if let Some(value) = value.as_str() {
        return value.to_owned();
    }

    value
        .as_object()
        .and_then(|object| object.keys().next())
        .cloned()
        .unwrap_or_default()
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
        index: field_usize(value, "idx"),
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
        layout_order: window_layout_order(value),
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

fn window_layout_order(value: &Value) -> Option<usize> {
    value
        .get("layout")
        .and_then(|layout| layout.get("pos_in_scrolling_layout"))
        .and_then(Value::as_array)
        .and_then(|position| position.first())
        .and_then(Value::as_i64)
        .and_then(|position| usize::try_from(position).ok())
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
                index: None,
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
    fn parses_window_layout_order_from_scrolling_layout() {
        let mut state = NiriEventState::default();

        let events = parse_niri_event(
            r#"{"WindowOpenedOrChanged":{"window":{"id":12,"workspace_id":6,"layout":{"pos_in_scrolling_layout":[42,0]}}}}"#,
            &mut state,
        );

        assert_eq!(
            events,
            vec![CompositorEvent::WindowChanged(Window {
                id: 12,
                title: None,
                app_id: None,
                pid: None,
                layout_order: Some(42),
                workspace: Some(6),
                focused: false,
                urgent: false,
                fullscreen: false,
                floating: None,
            })]
        );
    }

    #[test]
    fn parses_screencast_events() {
        let mut state = NiriEventState::default();

        let events = parse_niri_event(
            r#"{"CastStartedOrChanged":{"cast":{"stream_id":8,"session_id":5,"kind":"PipeWire","target":{"Output":"eDP-1"},"is_active":true,"pid":1234,"pw_node_id":42}}}"#,
            &mut state,
        );

        assert_eq!(
            events,
            vec![CompositorEvent::ScreencastChanged(ScreencastSession {
                id: "8".into(),
                session_id: Some("5".into()),
                kind: ScreencastKind::PipeWire,
                target: ScreencastTarget::Monitor,
                active: true,
                pipewire_node: Some(42),
                client_pid: Some(1234),
                stoppable: true,
            })]
        );

        assert_eq!(
            parse_niri_event(r#"{"CastStopped":{"stream_id":8}}"#, &mut state),
            vec![CompositorEvent::ScreencastStopped("8".into())]
        );
    }
}
