use std::time::Duration;

use crate::compositors;

const NOTIFICATION_FOCUS_DELAY: Duration = Duration::from_millis(75);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CompositorKind {
    Niri,
    Hyprland,
    #[default]
    Unknown,
}

impl CompositorKind {
    pub fn from_env<I, K, V>(vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut desktop = String::new();
        let mut session = String::new();
        let mut has_niri_socket = false;
        let mut has_hypr_sig = false;

        for (key, value) in vars {
            let key = key.into();
            let value = value.into();
            match key.as_str() {
                "XDG_CURRENT_DESKTOP" | "XDG_SESSION_DESKTOP" => desktop.push_str(&value),
                "DESKTOP_SESSION" => session.push_str(&value),
                "NIRI_SOCKET" => has_niri_socket = !value.is_empty(),
                "HYPRLAND_INSTANCE_SIGNATURE" => has_hypr_sig = !value.is_empty(),
                _ => {}
            }
        }

        let desktop = desktop.to_lowercase();
        let session = session.to_lowercase();

        if has_niri_socket || desktop.contains("niri") || session.contains("niri") {
            Self::Niri
        } else if has_hypr_sig || desktop.contains("hypr") || session.contains("hypr") {
            Self::Hyprland
        } else {
            Self::Unknown
        }
    }

    pub fn detect() -> Self {
        Self::from_env(std::env::vars())
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Niri => "niri",
            Self::Hyprland => "Hyprland",
            Self::Unknown => "Unknown compositor",
        }
    }

    pub fn live_source_label(self) -> &'static str {
        match self {
            Self::Niri => "niri msg --json outputs",
            Self::Hyprland => "hyprctl monitors -j",
            Self::Unknown => "GDK monitor fallback",
        }
    }

    pub fn apply_mode_label(self) -> &'static str {
        match self {
            Self::Niri => "Managed config fragment",
            Self::Hyprland => "Managed config fragment (planned)",
            Self::Unknown => "Unavailable",
        }
    }

    pub fn validation_label(self) -> &'static str {
        match self {
            Self::Niri => "niri validate -c",
            Self::Hyprland => "Not implemented yet",
            Self::Unknown => "Unavailable",
        }
    }

    pub fn reload_method_label(self) -> &'static str {
        match self {
            Self::Niri => "niri msg action load-config-file",
            Self::Hyprland => "Not implemented yet",
            Self::Unknown => "Unavailable",
        }
    }

    pub fn capabilities(self) -> CompositorCapabilities {
        match self {
            Self::Niri => CompositorCapabilities {
                workspace_listener: true,
                keyboard_listener: true,
                backdrop: true,
                night_light: true,
                night_light_per_output_control: true,
                switch_workspace: true,
                switch_workspace_relative: true,
                focus_window_relative: true,
                focus_window_by_id: true,
                focus_notification_target: true,
                switch_keyboard_layout_relative: true,
            },
            Self::Hyprland => CompositorCapabilities {
                workspace_listener: true,
                keyboard_listener: true,
                backdrop: false,
                night_light: true,
                night_light_per_output_control: true,
                switch_workspace: true,
                switch_workspace_relative: true,
                focus_window_relative: false,
                focus_window_by_id: false,
                focus_notification_target: false,
                switch_keyboard_layout_relative: true,
            },
            Self::Unknown => CompositorCapabilities::default(),
        }
    }
}

pub fn detect() -> CompositorKind {
    CompositorKind::detect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct CompositorCapabilities {
    pub workspace_listener: bool,
    pub keyboard_listener: bool,
    pub backdrop: bool,
    pub night_light: bool,
    pub night_light_per_output_control: bool,
    pub switch_workspace: bool,
    pub switch_workspace_relative: bool,
    pub focus_window_relative: bool,
    pub focus_window_by_id: bool,
    pub focus_notification_target: bool,
    pub switch_keyboard_layout_relative: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum CompositorListenerHealth {
    Starting,
    Ready,
    Unsupported,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspacePresentation {
    Workspaces,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkspaceSlot {
    pub index: u32,
    pub is_focused: bool,
    pub occupied: bool,
    pub is_urgent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkspaceWindow {
    pub id: u64,
    pub column: u32,
    pub is_focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkspaceSnapshot {
    pub presentation: WorkspacePresentation,
    pub current_workspace_index: Option<u32>,
    pub workspaces: Vec<WorkspaceSlot>,
    pub windows: Vec<WorkspaceWindow>,
}

impl Default for WorkspaceSnapshot {
    fn default() -> Self {
        Self {
            presentation: WorkspacePresentation::Workspaces,
            current_workspace_index: None,
            workspaces: Vec::new(),
            windows: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WorkspaceState {
    pub compositor: CompositorKind,
    pub capabilities: CompositorCapabilities,
    pub health: CompositorListenerHealth,
    pub snapshot: WorkspaceSnapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct KeyboardLayoutSnapshot {
    pub layout_names: Vec<String>,
    pub current_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct KeyboardLayoutState {
    pub compositor: CompositorKind,
    pub capabilities: CompositorCapabilities,
    pub health: CompositorListenerHealth,
    pub snapshot: KeyboardLayoutSnapshot,
}

pub fn short_layout_name(layout_name: &str) -> String {
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

pub async fn focus_notification_target(desktop_entry: Option<&str>, app_name: &str) -> bool {
    match detect() {
        CompositorKind::Niri => {
            let Some((window_id, workspace_index, already_focused)) =
                compositors::niri::notification_focus_target(desktop_entry, app_name).await
            else {
                return false;
            };

            if !already_focused {
                compositors::niri::switch_workspace(workspace_index).await;
                tokio::time::sleep(NOTIFICATION_FOCUS_DELAY).await;
            }

            compositors::niri::focus_window(window_id).await;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{CompositorKind, short_layout_name};

    #[test]
    fn compositor_detection_prefers_niri_markers() {
        let compositor = CompositorKind::from_env([
            ("XDG_CURRENT_DESKTOP", "niri"),
            ("DESKTOP_SESSION", "niri-uwsm"),
            ("NIRI_SOCKET", "/tmp/niri.sock"),
        ]);

        assert_eq!(compositor, CompositorKind::Niri);
    }

    #[test]
    fn short_layout_name_maps_known_languages() {
        assert_eq!(short_layout_name("English (US)"), "EN");
        assert_eq!(short_layout_name("Russian"), "RU");
        assert_eq!(short_layout_name("German"), "DE");
        assert_eq!(short_layout_name("Polish"), "PL");
        assert_eq!(short_layout_name("Georgian"), "GE");
    }

    #[test]
    fn short_layout_name_handles_raw_xkb_codes() {
        assert_eq!(short_layout_name("us"), "US");
        assert_eq!(short_layout_name("de_ch"), "DE_CH");
        assert_eq!(short_layout_name("ru"), "RU");
    }
}
