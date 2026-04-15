use std::{
    collections::HashMap,
    env,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::compositor::protocol::{
    KeyboardLayoutSnapshot, WorkspacePresentation, WorkspaceSlot, WorkspaceSnapshot,
};

pub(crate) async fn workspace_snapshot() -> Option<WorkspaceSnapshot> {
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
            Some(WorkspaceSlot {
                index: id as u32,
                is_focused: active_ids.contains(&id),
                occupied: windows > 0,
                is_urgent: false,
            })
        })
        .collect::<Vec<_>>();

    Some(WorkspaceSnapshot {
        presentation: WorkspacePresentation::Workspaces,
        current_workspace_index: workspaces
            .iter()
            .find(|ws| ws.is_focused)
            .map(|ws| ws.index),
        workspaces,
        windows: Vec::new(),
    })
}

pub(crate) async fn workspace_event_loop(tx: mpsc::Sender<()>) -> anyhow::Result<()> {
    let sig = match env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let socket_path = format!("{runtime_dir}/hypr/{sig}/.socket2.sock");

    tracing::info!("workspace service: connecting to hyprland event socket");

    let stream = UnixStream::connect(&socket_path).await?;
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();

    let _ = tx.send(()).await;

    while let Some(line) = lines.next_line().await? {
        if line.starts_with("workspace>>")
            || line.starts_with("workspacev2>>")
            || line.starts_with("createworkspace")
            || line.starts_with("destroyworkspace")
            || line.starts_with("focusedmon>>")
        {
            if tx.send(()).await.is_err() {
                break;
            }
        }
    }

    Ok(())
}

pub(crate) async fn switch_workspace(index: u32) {
    let _ = Command::new("hyprctl")
        .args(["dispatch", "workspace", &index.to_string()])
        .output()
        .await;
}

pub(crate) async fn switch_workspace_relative(next: bool) {
    let dir = if next { "+1" } else { "-1" };
    let _ = Command::new("hyprctl")
        .args(["dispatch", "workspace", dir])
        .output()
        .await;
}

pub(crate) async fn keyboard_snapshot() -> Option<KeyboardLayoutSnapshot> {
    let output = Command::new("hyprctl")
        .args(["devices", "-j"])
        .output()
        .await
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let keyboards = json.get("keyboards")?.as_array()?;
    let main_kb = keyboards
        .iter()
        .find(|kb| kb.get("main").and_then(|v| v.as_bool()).unwrap_or(false))?;
    let layout_str = main_kb.get("layout")?.as_str()?;
    let active_keymap = main_kb.get("active_keymap")?.as_str()?;

    let layout_codes: Vec<&str> = layout_str.split(',').collect();
    let active_index = find_active_index(&layout_codes, active_keymap);
    let layout_names = layout_codes
        .iter()
        .enumerate()
        .map(|(i, code)| {
            if i == active_index {
                active_keymap.to_string()
            } else {
                code.to_string()
            }
        })
        .collect();

    Some(KeyboardLayoutSnapshot {
        layout_names,
        current_index: active_index,
    })
}

pub(crate) async fn keyboard_event_loop(
    tx: mpsc::Sender<()>,
    per_window: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let sig = match env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let socket_path = format!("{runtime_dir}/hypr/{sig}/.socket2.sock");

    tracing::info!("keyboard layout service: connecting to hyprland event socket");

    let stream = UnixStream::connect(&socket_path).await?;
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    let mut window_layouts: HashMap<String, usize> = HashMap::new();
    let mut focused_window: Option<String> = None;

    let _ = tx.send(()).await;

    while let Some(line) = lines.next_line().await? {
        if line.starts_with("activelayout>>") {
            if per_window.load(Ordering::Relaxed) {
                if let (Some(wid), Some(state)) = (&focused_window, keyboard_snapshot().await) {
                    window_layouts.insert(wid.clone(), state.current_index);
                }
            }
            if tx.send(()).await.is_err() {
                break;
            }
        } else if per_window.load(Ordering::Relaxed) && line.starts_with("activewindowv2>>") {
            let addr = line.trim_start_matches("activewindowv2>>").to_string();
            if addr.is_empty() {
                focused_window = None;
                continue;
            }

            if let Some(ref old_wid) = focused_window {
                if let Some(state) = keyboard_snapshot().await {
                    window_layouts.insert(old_wid.clone(), state.current_index);
                }
            }
            focused_window = Some(addr.clone());
            let target_index = if let Some(&saved_index) = window_layouts.get(&addr) {
                saved_index
            } else {
                window_layouts.insert(addr, 0);
                0
            };
            let _ = Command::new("hyprctl")
                .args(["switchxkblayout", "all", &target_index.to_string()])
                .output()
                .await;
            if tx.send(()).await.is_err() {
                break;
            }
        } else if per_window.load(Ordering::Relaxed) && line.starts_with("closewindow>>") {
            let addr = line.trim_start_matches("closewindow>>").to_string();
            window_layouts.remove(&addr);
        }
    }

    Ok(())
}

pub(crate) async fn switch_layout_relative(next: bool) {
    let dir = if next { "next" } else { "prev" };
    let _ = Command::new("hyprctl")
        .args(["switchxkblayout", "all", dir])
        .output()
        .await;
}

fn find_active_index(layout_codes: &[&str], active_keymap: &str) -> usize {
    let keymap_lower = active_keymap.to_lowercase();
    let paren_content = keymap_lower.find('(').and_then(|start| {
        keymap_lower[start + 1..]
            .find(')')
            .map(|end| keymap_lower[start + 1..start + 1 + end].trim().to_string())
    });

    layout_codes
        .iter()
        .position(|code| {
            let code_lower = code.to_lowercase();
            if let Some(ref paren) = paren_content {
                if code_lower == *paren {
                    return true;
                }
            }
            if code_lower == keymap_lower {
                return true;
            }
            crate::compositor::protocol::short_layout_name(active_keymap).to_lowercase()
                == code_lower
        })
        .unwrap_or(0)
}
