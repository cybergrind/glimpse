use std::collections::HashMap;
use std::env;

use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Compositor {
    Hyprland,
    Niri,
}

#[derive(Debug, Clone)]
pub struct KeyboardState {
    pub layout_names: Vec<String>,
    pub current_index: usize,
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

pub fn short_name(layout_name: &str) -> String {
    // Try mapping as human-readable name first (single word or multi-word)
    let first_word = layout_name.split_whitespace().next().unwrap_or(layout_name);
    let code = match first_word.to_lowercase().as_str() {
        "english" => "EN",
        "russian" => "RU",
        "german" => "DE",
        "french" => "FR",
        "spanish" => "ES",
        "italian" => "IT",
        "portuguese" => "PT",
        "dutch" => "NL",
        "polish" => "PL",
        "czech" => "CZ",
        "slovak" => "SK",
        "hungarian" => "HU",
        "romanian" => "RO",
        "bulgarian" => "BG",
        "ukrainian" => "UA",
        "belarusian" => "BY",
        "serbian" => "RS",
        "croatian" => "HR",
        "slovenian" => "SI",
        "turkish" => "TR",
        "greek" => "GR",
        "arabic" => "AR",
        "hebrew" => "HE",
        "japanese" => "JP",
        "korean" => "KR",
        "chinese" => "CN",
        "thai" => "TH",
        "vietnamese" => "VN",
        "swedish" => "SE",
        "norwegian" => "NO",
        "danish" => "DK",
        "finnish" => "FI",
        "estonian" => "EE",
        "latvian" => "LV",
        "lithuanian" => "LT",
        "georgian" => "GE",
        _ => {
            // Raw xkb code (no spaces) — uppercase as-is: "us" → "US", "de_ch" → "DE_CH"
            if !layout_name.contains(' ') {
                return layout_name.to_uppercase();
            }
            return first_word
                .chars()
                .take(2)
                .collect::<String>()
                .to_uppercase();
        }
    };
    code.to_string()
}

async fn hyprland_query_state() -> Option<KeyboardState> {
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
    let layout_names: Vec<String> = layout_codes
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

    Some(KeyboardState {
        layout_names,
        current_index: active_index,
    })
}

fn find_active_index(layout_codes: &[&str], active_keymap: &str) -> usize {
    let keymap_lower = active_keymap.to_lowercase();
    // Extract parenthetical if present: "English (US)" → "us"
    let paren_content = keymap_lower.find('(').and_then(|start| {
        keymap_lower[start + 1..]
            .find(')')
            .map(|end| keymap_lower[start + 1..start + 1 + end].trim().to_string())
    });

    layout_codes
        .iter()
        .position(|code| {
            let code_lower = code.to_lowercase();
            // Exact match with parenthetical: "English (US)" matches "us"
            if let Some(ref paren) = paren_content {
                if code_lower == *paren {
                    return true;
                }
            }
            // Exact match: code equals full keymap
            if code_lower == keymap_lower {
                return true;
            }
            // Use short_name to resolve: "German" → "de", "Russian" → "ru"
            short_name(active_keymap).to_lowercase() == code_lower
        })
        .unwrap_or(0)
}

pub async fn hyprland_event_loop(tx: mpsc::Sender<KeyboardState>, per_window: bool) {
    let sig = match env::var("HYPRLAND_INSTANCE_SIGNATURE") {
        Ok(s) => s,
        Err(_) => return,
    };
    let runtime_dir = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let socket_path = format!("{runtime_dir}/hypr/{sig}/.socket2.sock");

    tracing::info!("keyboard: connecting to hyprland event socket");

    let stream = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("keyboard: hyprland socket connect failed: {e}");
            return;
        }
    };

    if let Some(state) = hyprland_query_state().await {
        if tx.send(state).await.is_err() {
            return;
        }
    }

    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    let mut window_layouts: HashMap<String, usize> = HashMap::new();
    let mut focused_window: Option<String> = None;

    while let Ok(Some(line)) = lines.next_line().await {
        if line.starts_with("activelayout>>") {
            if let Some(state) = hyprland_query_state().await {
                if per_window {
                    if let Some(ref wid) = focused_window {
                        window_layouts.insert(wid.clone(), state.current_index);
                    }
                }
                if tx.send(state).await.is_err() {
                    return;
                }
            }
        } else if per_window && line.starts_with("activewindowv2>>") {
            let addr = line.trim_start_matches("activewindowv2>>").to_string();
            if addr.is_empty() {
                focused_window = None;
                continue;
            }
            // Save current layout for the window we're leaving
            if let Some(ref old_wid) = focused_window {
                if let Some(state) = hyprland_query_state().await {
                    window_layouts.insert(old_wid.clone(), state.current_index);
                }
            }
            focused_window = Some(addr.clone());
            let target_index = if let Some(&saved_index) = window_layouts.get(&addr) {
                saved_index
            } else {
                // New window gets default layout (index 0)
                window_layouts.insert(addr, 0);
                0
            };
            let _ = Command::new("hyprctl")
                .args(["switchxkblayout", "all", &target_index.to_string()])
                .output()
                .await;
            if let Some(state) = hyprland_query_state().await {
                if tx.send(state).await.is_err() {
                    return;
                }
            }
        } else if per_window && line.starts_with("closewindow>>") {
            let addr = line.trim_start_matches("closewindow>>").to_string();
            window_layouts.remove(&addr);
        }
    }
}

async fn niri_query_state() -> Option<KeyboardState> {
    let output = Command::new("niri")
        .args(["msg", "-j", "keyboard-layouts"])
        .output()
        .await
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let names = json.get("names")?.as_array()?;
    let current_idx = json.get("current_idx")?.as_u64()? as usize;
    let layout_names: Vec<String> = names
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    Some(KeyboardState {
        layout_names,
        current_index: current_idx,
    })
}

pub async fn niri_event_loop(tx: mpsc::Sender<KeyboardState>, per_window: bool) {
    tracing::info!("keyboard: starting niri event stream");

    let mut child = match Command::new("niri")
        .args(["msg", "--json", "event-stream"])
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("keyboard: niri event-stream failed: {e}");
            return;
        }
    };

    let stdout = match child.stdout.take() {
        Some(s) => s,
        None => return,
    };

    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();
    let mut window_layouts: HashMap<u64, usize> = HashMap::new();
    let mut focused_window: Option<u64> = None;

    // Query and send initial state
    let mut current_index: usize = 0;
    if let Some(state) = niri_query_state().await {
        current_index = state.current_index;
        if tx.send(state).await.is_err() {
            let _ = child.kill().await;
            return;
        }
    }

    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        if event.get("KeyboardLayoutSwitched").is_some()
            || event.get("KeyboardLayoutsChanged").is_some()
        {
            if let Some(state) = niri_query_state().await {
                current_index = state.current_index;
                if per_window {
                    if let Some(wid) = focused_window {
                        window_layouts.insert(wid, current_index);
                    }
                }
                if tx.send(state).await.is_err() {
                    break;
                }
            }
        } else if per_window {
            if let Some(wf) = event.get("WindowFocusChanged") {
                let new_id = wf.get("id").and_then(|v| v.as_u64());
                // Save current layout for the window we're leaving
                if let Some(old_wid) = focused_window {
                    window_layouts.insert(old_wid, current_index);
                }
                focused_window = new_id;
                if let Some(wid) = new_id {
                    if let Some(&saved_index) = window_layouts.get(&wid) {
                        if saved_index != current_index {
                            let _ = Command::new("niri")
                                .args(["msg", "action", "switch-layout", &saved_index.to_string()])
                                .output()
                                .await;
                            current_index = saved_index;
                        }
                    } else {
                        // New window gets default layout (index 0)
                        window_layouts.insert(wid, 0);
                        if current_index != 0 {
                            let _ = Command::new("niri")
                                .args(["msg", "action", "switch-layout", "0"])
                                .output()
                                .await;
                            current_index = 0;
                        }
                    }
                }
            } else if let Some(wc) = event.get("WindowClosed") {
                if let Some(id) = wc.get("id").and_then(|v| v.as_u64()) {
                    window_layouts.remove(&id);
                }
            }
        }
    }

    let _ = child.kill().await;
}

pub async fn switch_layout_relative(compositor: Compositor, next: bool) {
    match compositor {
        Compositor::Hyprland => {
            let dir = if next { "next" } else { "prev" };
            tracing::info!("keyboard: switching layout {dir}");
            let _ = Command::new("hyprctl")
                .args(["switchxkblayout", "all", dir])
                .output()
                .await;
        }
        Compositor::Niri => {
            let dir = if next { "next" } else { "prev" };
            tracing::info!("keyboard: switching layout {dir}");
            let _ = Command::new("niri")
                .args(["msg", "action", "switch-layout", dir])
                .output()
                .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_maps_known_languages() {
        assert_eq!(short_name("English (US)"), "EN");
        assert_eq!(short_name("Russian"), "RU");
        assert_eq!(short_name("German"), "DE");
        assert_eq!(short_name("Polish"), "PL");
        assert_eq!(short_name("Georgian"), "GE");
    }

    #[test]
    fn short_name_handles_raw_xkb_codes() {
        assert_eq!(short_name("us"), "US");
        assert_eq!(short_name("de_ch"), "DE_CH");
        assert_eq!(short_name("ru"), "RU");
    }

    #[test]
    fn short_name_falls_back_to_first_two_chars() {
        assert_eq!(short_name("Klingon (Standard)"), "KL");
    }

    #[test]
    fn short_name_handles_empty_string() {
        assert_eq!(short_name(""), "");
    }

    #[test]
    fn find_active_index_matches_by_code_in_keymap() {
        let codes = vec!["us", "ru", "de"];
        assert_eq!(find_active_index(&codes, "English (US)"), 0);
        assert_eq!(find_active_index(&codes, "Russian"), 1);
        assert_eq!(find_active_index(&codes, "German"), 2);
    }

    #[test]
    fn find_active_index_defaults_to_zero_on_no_match() {
        let codes = vec!["us", "ru"];
        assert_eq!(find_active_index(&codes, "Unknown Layout"), 0);
    }
}
