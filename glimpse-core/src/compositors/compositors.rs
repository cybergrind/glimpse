use crate::compositors::{hyprland::Hyprland, niri::Niri};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compositor {
    Niri(Niri),
    Hyprland(Hyprland),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompositorType {
    #[default]
    Unsupported,
    Niri,
    Hyprland,
}

impl CompositorType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Niri => "niri",
            Self::Hyprland => "hyprland",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositorEvent {
    Snapshot(CompositorSnapshot),
    RefreshRequested(CompositorRefresh),
    WindowsChanged(Vec<Window>),
    WindowChanged(Window),
    WindowTitleChanged {
        window: usize,
        title: String,
    },
    WindowFullscreenChanged {
        window: Option<usize>,
        fullscreen: bool,
    },
    WindowFloatingChanged {
        window: usize,
        floating: bool,
    },
    WindowClosed(usize),
    WorkspacesChanged(Vec<Workspace>),
    WorkspaceChanged {
        id: usize,
        focused: bool,
    },
    WorkspaceActiveWindowChanged {
        workspace: usize,
        window: Option<usize>,
    },
    MonitorsChanged(Vec<Monitor>),
    MonitorChanged {
        name: String,
        active_workspace: Option<usize>,
        focused: bool,
    },
    KeyboardLayoutsChanged {
        layouts: Vec<KeyboardLayout>,
        current: Option<usize>,
    },
    KeyboardLayoutChanged {
        index: Option<usize>,
        name: Option<String>,
    },
    FocusedWindowChanged(Option<usize>),
    ScreencastsChanged(Vec<ScreencastSession>),
    ScreencastChanged(ScreencastSession),
    ScreencastStopped(String),
}

impl CompositorEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Snapshot(_) => "snapshot",
            Self::RefreshRequested(_) => "refresh-requested",
            Self::WindowsChanged(_) => "windows-changed",
            Self::WindowChanged(_) => "window-changed",
            Self::WindowTitleChanged { .. } => "window-title-changed",
            Self::WindowFullscreenChanged { .. } => "window-fullscreen-changed",
            Self::WindowFloatingChanged { .. } => "window-floating-changed",
            Self::WindowClosed(_) => "window-closed",
            Self::WorkspacesChanged(_) => "workspaces-changed",
            Self::WorkspaceChanged { .. } => "workspace-changed",
            Self::WorkspaceActiveWindowChanged { .. } => "workspace-active-window-changed",
            Self::MonitorsChanged(_) => "monitors-changed",
            Self::MonitorChanged { .. } => "monitor-changed",
            Self::KeyboardLayoutsChanged { .. } => "keyboard-layouts-changed",
            Self::KeyboardLayoutChanged { .. } => "keyboard-layout-changed",
            Self::FocusedWindowChanged(_) => "focused-window-changed",
            Self::ScreencastsChanged(_) => "screencasts-changed",
            Self::ScreencastChanged(_) => "screencast-changed",
            Self::ScreencastStopped(_) => "screencast-stopped",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompositorRefresh {
    full: bool,
    structure: bool,
    keyboard_layouts: bool,
}

impl CompositorRefresh {
    pub const FULL: Self = Self {
        full: true,
        structure: false,
        keyboard_layouts: false,
    };
    pub const STRUCTURE: Self = Self {
        full: false,
        structure: true,
        keyboard_layouts: false,
    };
    pub const KEYBOARD_LAYOUTS: Self = Self {
        full: false,
        structure: false,
        keyboard_layouts: true,
    };

    pub fn merge(self, other: Self) -> Self {
        if self.full || other.full {
            return Self::FULL;
        }

        Self {
            full: false,
            structure: self.structure || other.structure,
            keyboard_layouts: self.keyboard_layouts || other.keyboard_layouts,
        }
    }

    pub fn is_full(self) -> bool {
        self.full
    }

    pub fn includes_structure(self) -> bool {
        self.structure
    }

    pub fn includes_keyboard_layouts(self) -> bool {
        self.keyboard_layouts
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::CompositorRefresh;

    #[test]
    fn partial_refreshes_merge_without_escalating_to_full() {
        let refresh = CompositorRefresh::STRUCTURE.merge(CompositorRefresh::KEYBOARD_LAYOUTS);

        assert!(!refresh.is_full());
        assert!(refresh.includes_structure());
        assert!(refresh.includes_keyboard_layouts());
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompositorSnapshot {
    pub capabilities: CompositorCapabilities,
    pub windows: Vec<Window>,
    pub workspaces: Vec<Workspace>,
    pub monitors: Vec<Monitor>,
    pub screencasts: Vec<ScreencastSession>,
    pub keyboard_layouts: Vec<KeyboardLayout>,
    pub current_keyboard_layout: Option<usize>,
    pub focused_window: Option<usize>,
    pub current_workspace: Option<usize>,
}

impl CompositorSnapshot {
    pub fn into_structure(self) -> CompositorStructureSnapshot {
        CompositorStructureSnapshot {
            windows: self.windows,
            workspaces: self.workspaces,
            monitors: self.monitors,
            focused_window: self.focused_window,
            current_workspace: self.current_workspace,
        }
    }

    pub fn into_keyboard_layouts(self) -> KeyboardLayoutSnapshot {
        KeyboardLayoutSnapshot {
            keyboard_layouts: self.keyboard_layouts,
            current_keyboard_layout: self.current_keyboard_layout,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompositorStructureSnapshot {
    pub windows: Vec<Window>,
    pub workspaces: Vec<Workspace>,
    pub monitors: Vec<Monitor>,
    pub focused_window: Option<usize>,
    pub current_workspace: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyboardLayoutSnapshot {
    pub keyboard_layouts: Vec<KeyboardLayout>,
    pub current_keyboard_layout: Option<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompositorCapabilities {
    pub windows: bool,
    pub workspaces: bool,
    pub monitors: bool,
    pub keyboard_layouts: bool,
    pub focused_window: bool,
    pub current_workspace: bool,
    pub fullscreen: bool,
    pub floating: bool,
    pub window_titles: bool,
    pub night_light: bool,
    pub screencast_state: ScreencastStateCapability,
    pub screencast_control: ScreencastControlCapability,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScreencastStateCapability {
    #[default]
    None,
    ActiveKind,
    Sessions,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScreencastControlCapability {
    #[default]
    None,
    StopSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreencastKind {
    PipeWire,
    WlrScreencopy,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreencastTarget {
    Monitor,
    Window,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreencastSession {
    pub id: String,
    pub session_id: Option<String>,
    pub kind: ScreencastKind,
    pub target: ScreencastTarget,
    pub active: bool,
    pub pipewire_node: Option<u32>,
    pub client_pid: Option<i32>,
    pub stoppable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub id: usize,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub pid: Option<i32>,
    pub layout_order: Option<usize>,
    pub workspace: Option<usize>,
    pub focused: bool,
    pub urgent: bool,
    pub fullscreen: bool,
    pub floating: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: usize,
    pub index: Option<usize>,
    pub name: Option<String>,
    pub monitor: Option<String>,
    pub active: bool,
    pub focused: bool,
    pub urgent: bool,
    pub active_window: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Monitor {
    pub id: Option<usize>,
    pub name: String,
    pub description: Option<String>,
    pub active_workspace: Option<usize>,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyboardLayout {
    pub index: usize,
    pub name: String,
}

impl Compositor {
    pub fn compositor_type(&self) -> CompositorType {
        match self {
            Self::Niri(_) => CompositorType::Niri,
            Self::Hyprland(_) => CompositorType::Hyprland,
        }
    }

    pub fn name(&self) -> &'static str {
        self.compositor_type().name()
    }

    pub async fn listen(self, sender: mpsc::Sender<CompositorEvent>) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.listen(sender).await,
            Self::Hyprland(compositor) => compositor.listen(sender).await,
        }
    }

    pub async fn snapshot(&self) -> anyhow::Result<CompositorSnapshot> {
        match self {
            Self::Niri(compositor) => compositor.snapshot().await,
            Self::Hyprland(compositor) => compositor.snapshot().await,
        }
    }

    pub async fn structure_snapshot(&self) -> anyhow::Result<CompositorStructureSnapshot> {
        match self {
            Self::Niri(compositor) => compositor
                .snapshot()
                .await
                .map(CompositorSnapshot::into_structure),
            Self::Hyprland(compositor) => compositor.structure_snapshot().await,
        }
    }

    pub async fn keyboard_layout_snapshot(&self) -> anyhow::Result<KeyboardLayoutSnapshot> {
        match self {
            Self::Niri(compositor) => compositor
                .snapshot()
                .await
                .map(CompositorSnapshot::into_keyboard_layouts),
            Self::Hyprland(compositor) => compositor.keyboard_layout_snapshot().await,
        }
    }

    pub fn capabilities(&self) -> CompositorCapabilities {
        match self {
            Self::Niri(compositor) => compositor.capabilities(),
            Self::Hyprland(compositor) => compositor.capabilities(),
        }
    }

    pub async fn set_keyboard_layout(&self, layout: usize) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.set_keyboard_layout(layout).await,
            Self::Hyprland(compositor) => compositor.set_keyboard_layout(layout).await,
        }
    }

    pub async fn set_workspace(&self, workspace: usize) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.set_workspace(workspace).await,
            Self::Hyprland(compositor) => compositor.set_workspace(workspace).await,
        }
    }

    pub async fn focus_next_workspace(&self) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.focus_next_workspace().await,
            Self::Hyprland(compositor) => compositor.focus_next_workspace().await,
        }
    }

    pub async fn focus_previous_workspace(&self) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.focus_previous_workspace().await,
            Self::Hyprland(compositor) => compositor.focus_previous_workspace().await,
        }
    }

    pub async fn focus_window(&self, window: usize) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.focus_window(window).await,
            Self::Hyprland(compositor) => compositor.focus_window(window).await,
        }
    }

    pub async fn focus_next_window(&self) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.focus_next_window().await,
            Self::Hyprland(compositor) => compositor.focus_next_window().await,
        }
    }

    pub async fn focus_previous_window(&self) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.focus_previous_window().await,
            Self::Hyprland(compositor) => compositor.focus_previous_window().await,
        }
    }

    pub async fn stop_screencast(&self, session_id: &str) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.stop_screencast(session_id).await,
            Self::Hyprland(_) => anyhow::bail!("hyprland does not support stopping screencasts"),
        }
    }
}

pub fn detect_compositor() -> Option<Compositor> {
    detect_compositor_from_env(std::env::vars())
}

fn detect_compositor_from_env<I, K, V>(vars: I) -> Option<Compositor>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mut has_niri_socket = false;
    let mut has_hyprland_signature = false;
    let mut has_niri_session = false;
    let mut has_hyprland_session = false;

    for (key, value) in vars {
        let key = key.as_ref();
        let value = value.as_ref();

        match key {
            "NIRI_SOCKET" => has_niri_socket = !value.is_empty(),
            "HYPRLAND_INSTANCE_SIGNATURE" => has_hyprland_signature = !value.is_empty(),
            "XDG_CURRENT_DESKTOP" | "XDG_SESSION_DESKTOP" | "DESKTOP_SESSION" => {
                let value = value.to_ascii_lowercase();
                has_niri_session |= value.contains("niri");
                has_hyprland_session |= value.contains("hyprland") || value.contains("hypr");
            }
            _ => {}
        }
    }

    if has_niri_socket || has_niri_session {
        Some(Compositor::Niri(Niri))
    } else if has_hyprland_signature || has_hyprland_session {
        Some(Compositor::Hyprland(Hyprland))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_niri_from_socket_or_desktop() {
        assert_eq!(
            detect_compositor_from_env([("NIRI_SOCKET", "/run/user/1000/niri.sock")])
                .map(|compositor| compositor.name()),
            Some("niri")
        );
        assert_eq!(
            detect_compositor_from_env([("XDG_CURRENT_DESKTOP", "niri")])
                .map(|compositor| compositor.name()),
            Some("niri")
        );
    }

    #[test]
    fn detects_hyprland_from_signature_or_desktop() {
        assert_eq!(
            detect_compositor_from_env([("HYPRLAND_INSTANCE_SIGNATURE", "abc")])
                .map(|compositor| compositor.name()),
            Some("hyprland")
        );
        assert_eq!(
            detect_compositor_from_env([("XDG_CURRENT_DESKTOP", "Hyprland")])
                .map(|compositor| compositor.name()),
            Some("hyprland")
        );
    }

    #[test]
    fn unsupported_sessions_return_none() {
        assert_eq!(
            detect_compositor_from_env([("XDG_CURRENT_DESKTOP", "GNOME")]),
            None
        );
    }
}
