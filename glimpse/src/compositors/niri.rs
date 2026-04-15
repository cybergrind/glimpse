use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::compositor::protocol::{
    KeyboardLayoutSnapshot, WorkspacePresentation, WorkspaceSnapshot, WorkspaceWindow,
};

pub(crate) async fn workspace_snapshot() -> Option<WorkspaceSnapshot> {
    let (ws_output, win_output) = tokio::join!(
        Command::new("niri")
            .args(["msg", "-j", "workspaces"])
            .output(),
        Command::new("niri").args(["msg", "-j", "windows"]).output(),
    );

    let ws_json: Vec<serde_json::Value> = serde_json::from_slice(&ws_output.ok()?.stdout).ok()?;
    let win_json: Vec<serde_json::Value> = serde_json::from_slice(&win_output.ok()?.stdout).ok()?;

    parse_niri_window_state(&ws_json, &win_json)
}

pub(crate) async fn workspace_event_loop(tx: mpsc::Sender<()>) -> anyhow::Result<()> {
    tracing::info!("workspace service: starting niri event stream");

    let mut child = Command::new("niri")
        .args(["msg", "--json", "event-stream"])
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("niri event stream missing stdout"))?;
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let _ = tx.send(()).await;

    while let Some(line) = lines.next_line().await? {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        let relevant = event.get("WorkspacesChanged").is_some()
            || event.get("WorkspaceActivated").is_some()
            || event.get("WindowsChanged").is_some()
            || event.get("WindowOpenedOrChanged").is_some()
            || event.get("WindowClosed").is_some()
            || event.get("WindowFocusChanged").is_some();

        if relevant && tx.send(()).await.is_err() {
            break;
        }
    }

    let _ = child.kill().await;
    Ok(())
}

pub(crate) async fn switch_workspace(index: u32) {
    let _ = Command::new("niri")
        .args(["msg", "action", "focus-workspace", &index.to_string()])
        .output()
        .await;
}

pub(crate) async fn switch_workspace_relative(next: bool) {
    let action = if next {
        "focus-workspace-down"
    } else {
        "focus-workspace-up"
    };
    let _ = Command::new("niri")
        .args(["msg", "action", action])
        .output()
        .await;
}

pub(crate) async fn focus_window_relative(next: bool) {
    let action = if next {
        "focus-column-right"
    } else {
        "focus-column-left"
    };
    let _ = Command::new("niri")
        .args(["msg", "action", action])
        .output()
        .await;
}

pub(crate) async fn focus_window(id: u64) {
    let _ = Command::new("niri")
        .args(["msg", "action", "focus-window", "--id", &id.to_string()])
        .output()
        .await;
}

pub(crate) async fn keyboard_snapshot() -> Option<KeyboardLayoutSnapshot> {
    let output = Command::new("niri")
        .args(["msg", "-j", "keyboard-layouts"])
        .output()
        .await
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let names = json.get("names")?.as_array()?;
    let current_idx = json.get("current_idx")?.as_u64()? as usize;
    let layout_names = names
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    Some(KeyboardLayoutSnapshot {
        layout_names,
        current_index: current_idx,
    })
}

pub(crate) async fn keyboard_event_loop(
    tx: mpsc::Sender<()>,
    per_window: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    tracing::info!("keyboard layout service: starting niri event stream");

    let mut child = Command::new("niri")
        .args(["msg", "--json", "event-stream"])
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("niri keyboard event stream missing stdout"))?;
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut window_layouts: HashMap<u64, usize> = HashMap::new();
    let mut focused_window: Option<u64> = None;
    let mut current_index = keyboard_snapshot()
        .await
        .map_or(0, |snapshot| snapshot.current_index);

    let _ = tx.send(()).await;

    while let Some(line) = lines.next_line().await? {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        if event.get("KeyboardLayoutSwitched").is_some()
            || event.get("KeyboardLayoutsChanged").is_some()
        {
            if let Some(state) = keyboard_snapshot().await {
                current_index = state.current_index;
                if per_window.load(Ordering::Relaxed) {
                    if let Some(window_id) = focused_window {
                        window_layouts.insert(window_id, current_index);
                    }
                }
            }
            if tx.send(()).await.is_err() {
                break;
            }
        } else if per_window.load(Ordering::Relaxed) {
            if let Some(wf) = event.get("WindowFocusChanged") {
                let new_id = wf.get("id").and_then(|v| v.as_u64());
                if let Some(old_wid) = focused_window {
                    window_layouts.insert(old_wid, current_index);
                }
                focused_window = new_id;
                if let Some(window_id) = new_id {
                    if let Some(&saved_index) = window_layouts.get(&window_id) {
                        if saved_index != current_index {
                            let _ = Command::new("niri")
                                .args(["msg", "action", "switch-layout", &saved_index.to_string()])
                                .output()
                                .await;
                            current_index = saved_index;
                        }
                    } else {
                        window_layouts.insert(window_id, 0);
                        if current_index != 0 {
                            let _ = Command::new("niri")
                                .args(["msg", "action", "switch-layout", "0"])
                                .output()
                                .await;
                            current_index = 0;
                        }
                    }
                    if tx.send(()).await.is_err() {
                        break;
                    }
                }
            } else if let Some(window_closed) = event.get("WindowClosed") {
                if let Some(id) = window_closed.get("id").and_then(|v| v.as_u64()) {
                    window_layouts.remove(&id);
                }
            }
        }
    }

    let _ = child.kill().await;
    Ok(())
}

pub(crate) async fn switch_layout_relative(next: bool) {
    let dir = if next { "next" } else { "prev" };
    let _ = Command::new("niri")
        .args(["msg", "action", "switch-layout", dir])
        .output()
        .await;
}

pub(crate) async fn notification_focus_target(
    desktop_entry: Option<&str>,
    app_name: &str,
) -> Option<(u64, u32, bool)> {
    let (ws_output, win_output) = tokio::join!(
        Command::new("niri")
            .args(["msg", "-j", "workspaces"])
            .output(),
        Command::new("niri").args(["msg", "-j", "windows"]).output(),
    );

    let (Ok(ws_output), Ok(win_output)) = (ws_output, win_output) else {
        return None;
    };

    let (Ok(workspaces), Ok(windows)) = (
        serde_json::from_slice::<Vec<serde_json::Value>>(&ws_output.stdout),
        serde_json::from_slice::<Vec<serde_json::Value>>(&win_output.stdout),
    ) else {
        return None;
    };

    let Some((window_id, workspace_id)) =
        select_window_for_notification(&windows, desktop_entry, app_name)
    else {
        return None;
    };

    let workspace_indexes: HashMap<u64, u32> = workspaces
        .iter()
        .filter_map(|workspace| {
            Some((
                workspace.get("id")?.as_u64()?,
                workspace.get("idx")?.as_u64()? as u32,
            ))
        })
        .collect();

    let focused_workspace_id = workspaces.iter().find_map(|workspace| {
        workspace
            .get("is_focused")
            .and_then(|value| value.as_bool())
            .filter(|is_focused| *is_focused)
            .and_then(|_| workspace.get("id")?.as_u64())
    });

    let workspace_index = workspace_indexes.get(&workspace_id).copied()?;
    Some((
        window_id,
        workspace_index,
        focused_workspace_id == Some(workspace_id),
    ))
}

fn parse_niri_window_state(
    ws_json: &[serde_json::Value],
    win_json: &[serde_json::Value],
) -> Option<WorkspaceSnapshot> {
    let focused_ws = ws_json.iter().find(|ws| {
        ws.get("is_focused")
            .and_then(|f| f.as_bool())
            .unwrap_or(false)
    })?;
    let focused_ws_id = focused_ws.get("id")?.as_u64()?;
    let workspace_index = focused_ws.get("idx")?.as_u64()? as u32;

    let mut windows = win_json
        .iter()
        .filter_map(|w| {
            let ws_id = w.get("workspace_id")?.as_u64()?;
            if ws_id != focused_ws_id {
                return None;
            }
            Some(WorkspaceWindow {
                id: w.get("id")?.as_u64()?,
                is_focused: w
                    .get("is_focused")
                    .and_then(|f| f.as_bool())
                    .unwrap_or(false),
                column: w
                    .get("layout")
                    .and_then(|l| l.get("pos_in_scrolling_layout"))
                    .and_then(|p| p.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            })
        })
        .collect::<Vec<_>>();
    windows.sort_by_key(|w| w.column);

    Some(WorkspaceSnapshot {
        presentation: WorkspacePresentation::Windows,
        current_workspace_index: Some(workspace_index),
        workspaces: Vec::new(),
        windows,
    })
}

fn normalize_app_id(value: &str) -> String {
    value
        .strip_suffix(".desktop")
        .unwrap_or(value)
        .trim()
        .to_ascii_lowercase()
}

fn notification_target_match_score(
    app_id: Option<&str>,
    desktop_entry: Option<&str>,
    app_name: &str,
) -> Option<u8> {
    let app_id = normalize_app_id(app_id?.trim());
    let desktop_entry = desktop_entry
        .map(normalize_app_id)
        .filter(|value| !value.is_empty());
    let app_name = normalize_app_id(app_name);

    if let Some(entry) = desktop_entry {
        if app_id == entry {
            return Some(3);
        }
    }
    if !app_name.is_empty() && app_id == app_name {
        return Some(2);
    }
    None
}

fn select_window_for_notification(
    windows: &[serde_json::Value],
    desktop_entry: Option<&str>,
    app_name: &str,
) -> Option<(u64, u64)> {
    windows
        .iter()
        .filter_map(|window| {
            let id = window.get("id")?.as_u64()?;
            let workspace_id = window.get("workspace_id")?.as_u64()?;
            let score = notification_target_match_score(
                window.get("app_id").and_then(|value| value.as_str()),
                desktop_entry,
                app_name,
            )?;
            Some((score, id, workspace_id))
        })
        .max_by_key(|(score, _, _)| *score)
        .map(|(_, id, workspace_id)| (id, workspace_id))
}

#[cfg(test)]
mod tests {
    use super::{normalize_app_id, parse_niri_window_state, select_window_for_notification};

    #[test]
    fn parse_niri_windows_filters_to_focused_workspace() {
        let ws = vec![
            serde_json::json!({"id": 1, "idx": 1, "is_focused": false}),
            serde_json::json!({"id": 2, "idx": 2, "is_focused": true}),
        ];
        let win = vec![
            serde_json::json!({"id": 10, "workspace_id": 1, "is_focused": false, "layout": {"pos_in_scrolling_layout": [1, 1]}}),
            serde_json::json!({"id": 20, "workspace_id": 2, "is_focused": true, "layout": {"pos_in_scrolling_layout": [1, 1]}}),
            serde_json::json!({"id": 21, "workspace_id": 2, "is_focused": false, "layout": {"pos_in_scrolling_layout": [2, 1]}}),
        ];

        let state = parse_niri_window_state(&ws, &win).unwrap();

        assert_eq!(state.current_workspace_index, Some(2));
        assert_eq!(state.windows.len(), 2);
        assert_eq!(state.windows[0].id, 20);
        assert!(state.windows[0].is_focused);
        assert_eq!(state.windows[1].id, 21);
        assert!(!state.windows[1].is_focused);
    }

    #[test]
    fn parse_niri_windows_sorts_by_column() {
        let ws = vec![serde_json::json!({"id": 1, "idx": 1, "is_focused": true})];
        let win = vec![
            serde_json::json!({"id": 3, "workspace_id": 1, "is_focused": false, "layout": {"pos_in_scrolling_layout": [3, 1]}}),
            serde_json::json!({"id": 1, "workspace_id": 1, "is_focused": false, "layout": {"pos_in_scrolling_layout": [1, 1]}}),
            serde_json::json!({"id": 2, "workspace_id": 1, "is_focused": true, "layout": {"pos_in_scrolling_layout": [2, 1]}}),
        ];

        let state = parse_niri_window_state(&ws, &win).unwrap();

        assert_eq!(state.windows[0].id, 1);
        assert_eq!(state.windows[1].id, 2);
        assert_eq!(state.windows[2].id, 3);
    }

    #[test]
    fn strips_desktop_suffix_when_normalizing() {
        assert_eq!(normalize_app_id("firefox.desktop"), "firefox");
        assert_eq!(
            normalize_app_id("org.mozilla.firefox"),
            "org.mozilla.firefox"
        );
    }

    #[test]
    fn prefers_exact_desktop_entry_match() {
        let windows = vec![
            serde_json::json!({"id": 1, "workspace_id": 10, "app_id": "slack"}),
            serde_json::json!({"id": 2, "workspace_id": 20, "app_id": "firefox"}),
        ];

        let selected = select_window_for_notification(&windows, Some("firefox.desktop"), "Firefox");

        assert_eq!(selected, Some((2, 20)));
    }
}
