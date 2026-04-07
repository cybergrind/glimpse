use std::collections::HashMap;
use std::env;

use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Compositor {
    Hyprland,
    Niri,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub index: u32,
    pub is_focused: bool,
    pub occupied: bool,
    pub is_urgent: bool,
}

#[derive(Debug, Clone)]
pub struct WorkspaceState {
    pub workspaces: Vec<Workspace>,
}

#[derive(Debug, Clone)]
pub struct NiriWindow {
    pub id: u64,
    pub column: u32,
    pub is_focused: bool,
}

#[derive(Debug, Clone)]
pub struct NiriWindowState {
    pub workspace_index: u32,
    pub windows: Vec<NiriWindow>,
}

#[derive(Debug, Clone)]
pub enum AppletState {
    Hyprland(WorkspaceState),
    Niri(NiriWindowState),
}

pub fn detect() -> Option<Compositor> {
    if env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        Some(Compositor::Hyprland)
    } else if env::var("NIRI_SOCKET").is_ok() {
        Some(Compositor::Niri)
    } else {
        None
    }
}

// --- Hyprland ---

async fn hyprland_query_state() -> Option<WorkspaceState> {
    let ws_output = Command::new("hyprctl")
        .args(["workspaces", "-j"])
        .output()
        .await
        .ok()?;
    let mon_output = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .await
        .ok()?;

    let ws_json: Vec<serde_json::Value> = serde_json::from_slice(&ws_output.stdout).ok()?;
    let mon_json: Vec<serde_json::Value> = serde_json::from_slice(&mon_output.stdout).ok()?;

    let active_ids: Vec<i64> = mon_json
        .iter()
        .filter_map(|m| m.get("activeWorkspace")?.get("id")?.as_i64())
        .collect();

    let workspaces = ws_json
        .iter()
        .filter_map(|ws| {
            let id = ws.get("id")?.as_i64()?;
            if id < 0 {
                return None;
            }
            let windows = ws.get("windows").and_then(|w| w.as_i64()).unwrap_or(0);
            Some(Workspace {
                index: id as u32,
                is_focused: active_ids.contains(&id),
                occupied: windows > 0,
                is_urgent: false,
            })
        })
        .collect();

    Some(WorkspaceState { workspaces })
}

pub async fn hyprland_event_loop(tx: mpsc::Sender<AppletState>) {
    let sig = match env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        Ok(s) => s,
        Err(_) => return,
    };
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let socket_path = format!("{runtime_dir}/hypr/{sig}/.socket2.sock");

    tracing::info!("workspaces: connecting to hyprland event socket");

    let stream = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("workspaces: hyprland socket connect failed: {e}");
            return;
        }
    };

    if let Some(state) = hyprland_query_state().await {
        if tx.send(AppletState::Hyprland(state)).await.is_err() {
            return;
        }
    }

    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.starts_with("workspace>>")
            || line.starts_with("workspacev2>>")
            || line.starts_with("createworkspace")
            || line.starts_with("destroyworkspace")
            || line.starts_with("focusedmon>>")
        {
            if let Some(state) = hyprland_query_state().await {
                if tx.send(AppletState::Hyprland(state)).await.is_err() {
                    return;
                }
            }
        }
    }
}

// --- Niri ---

fn parse_niri_window_state(
    ws_json: &[serde_json::Value],
    win_json: &[serde_json::Value],
) -> Option<NiriWindowState> {
    let focused_ws = ws_json.iter().find(|ws| {
        ws.get("is_focused")
            .and_then(|f| f.as_bool())
            .unwrap_or(false)
    })?;
    let focused_ws_id = focused_ws.get("id")?.as_u64()?;
    let workspace_index = focused_ws.get("idx")?.as_u64()? as u32;

    let mut windows: Vec<NiriWindow> = win_json
        .iter()
        .filter_map(|w| {
            let ws_id = w.get("workspace_id")?.as_u64()?;
            if ws_id != focused_ws_id {
                return None;
            }
            let id = w.get("id")?.as_u64()?;
            let is_focused = w
                .get("is_focused")
                .and_then(|f| f.as_bool())
                .unwrap_or(false);
            let column = w
                .get("layout")
                .and_then(|l| l.get("pos_in_scrolling_layout"))
                .and_then(|p| p.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            Some(NiriWindow {
                id,
                column,
                is_focused,
            })
        })
        .collect();
    windows.sort_by_key(|w| w.column);

    Some(NiriWindowState {
        workspace_index,
        windows,
    })
}

async fn niri_query_full_state() -> Option<NiriWindowState> {
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

pub async fn niri_event_loop(tx: mpsc::Sender<AppletState>) {
    tracing::info!("workspaces: starting niri event stream");

    let mut child = match Command::new("niri")
        .args(["msg", "--json", "event-stream"])
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("workspaces: niri event-stream failed: {e}");
            return;
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => return,
    };

    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut last_state_windows: Vec<NiriWindow> = Vec::new();

    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        let dominated = event.get("WorkspacesChanged").is_some()
            || event.get("WorkspaceActivated").is_some()
            || event.get("WindowsChanged").is_some()
            || event.get("WindowOpenedOrChanged").is_some()
            || event.get("WindowClosed").is_some()
            || event.get("WindowFocusChanged").is_some();

        if dominated {
            if let Some(state) = niri_query_full_state().await {
                // If no window is focused (e.g. mouse moved to panel),
                // keep the previous active marker
                let any_focused = state.windows.iter().any(|w| w.is_focused);
                if !any_focused && !last_state_windows.is_empty() {
                    continue;
                }
                last_state_windows = state.windows.clone();
                if tx.send(AppletState::Niri(state)).await.is_err() {
                    break;
                }
            }
        }
    }

    let _ = child.kill().await;
}

// --- Actions ---

pub async fn switch_workspace(compositor: Compositor, index: u32) {
    match compositor {
        Compositor::Hyprland => {
            tracing::info!("workspaces: switching to workspace {index}");
            let _ = Command::new("hyprctl")
                .args(["dispatch", "workspace", &index.to_string()])
                .output()
                .await;
        }
        Compositor::Niri => {
            tracing::info!("workspaces: focusing workspace {index}");
            let _ = Command::new("niri")
                .args(["msg", "action", "focus-workspace", &index.to_string()])
                .output()
                .await;
        }
    }
}

pub async fn switch_workspace_relative(compositor: Compositor, next: bool) {
    match compositor {
        Compositor::Hyprland => {
            let dir = if next { "+1" } else { "-1" };
            tracing::info!("workspaces: switching workspace {dir}");
            let _ = Command::new("hyprctl")
                .args(["dispatch", "workspace", dir])
                .output()
                .await;
        }
        Compositor::Niri => {
            let action = if next {
                "focus-workspace-down"
            } else {
                "focus-workspace-up"
            };
            tracing::info!("workspaces: {action}");
            let _ = Command::new("niri")
                .args(["msg", "action", action])
                .output()
                .await;
        }
    }
}

pub async fn focus_window_relative(_compositor: Compositor, next: bool) {
    let action = if next {
        "focus-column-right"
    } else {
        "focus-column-left"
    };
    tracing::info!("workspaces: {action}");
    let _ = Command::new("niri")
        .args(["msg", "action", action])
        .output()
        .await;
}

pub async fn focus_window(id: u64) {
    tracing::info!("workspaces: focusing window {id}");
    let _ = Command::new("niri")
        .args(["msg", "action", "focus-window", "--id", &id.to_string()])
        .output()
        .await;
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

fn select_niri_window_for_notification(
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

pub async fn focus_notification_target(desktop_entry: Option<&str>, app_name: &str) -> bool {
    if detect() != Some(Compositor::Niri) {
        return false;
    }

    let (ws_output, win_output) = tokio::join!(
        Command::new("niri")
            .args(["msg", "-j", "workspaces"])
            .output(),
        Command::new("niri").args(["msg", "-j", "windows"]).output(),
    );

    let (Ok(ws_output), Ok(win_output)) = (ws_output, win_output) else {
        return false;
    };

    let (Ok(workspaces), Ok(windows)) = (
        serde_json::from_slice::<Vec<serde_json::Value>>(&ws_output.stdout),
        serde_json::from_slice::<Vec<serde_json::Value>>(&win_output.stdout),
    ) else {
        return false;
    };

    let Some((window_id, workspace_id)) =
        select_niri_window_for_notification(&windows, desktop_entry, app_name)
    else {
        return false;
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

    if focused_workspace_id != Some(workspace_id) {
        if let Some(index) = workspace_indexes.get(&workspace_id).copied() {
            switch_workspace(Compositor::Niri, index).await;
            sleep(Duration::from_millis(75)).await;
        }
    }

    focus_window(window_id).await;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert_eq!(state.workspace_index, 2);
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
    fn parse_niri_windows_returns_none_when_no_focused_workspace() {
        let ws = vec![serde_json::json!({"id": 1, "idx": 1, "is_focused": false})];
        let win = vec![];

        assert!(parse_niri_window_state(&ws, &win).is_none());
    }

    #[test]
    fn parse_niri_windows_empty_workspace_returns_empty_list() {
        let ws = vec![serde_json::json!({"id": 1, "idx": 1, "is_focused": true})];
        let win = vec![];

        let state = parse_niri_window_state(&ws, &win).unwrap();

        assert_eq!(state.workspace_index, 1);
        assert!(state.windows.is_empty());
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

        let selected =
            select_niri_window_for_notification(&windows, Some("firefox.desktop"), "Firefox");

        assert_eq!(selected, Some((2, 20)));
    }

    #[test]
    fn falls_back_to_app_name_match() {
        let windows =
            vec![serde_json::json!({"id": 9, "workspace_id": 30, "app_id": "thunderbird"})];

        let selected = select_niri_window_for_notification(&windows, None, "Thunderbird");

        assert_eq!(selected, Some((9, 30)));
    }
}
