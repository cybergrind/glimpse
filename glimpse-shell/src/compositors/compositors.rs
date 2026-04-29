use crate::compositors::{hyprland::Hyprland, niri::Niri};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compositor {
    Niri(Niri),
    Hyprland(Hyprland),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompositorEvent {
    FocusedWindowChanged { workspace: usize, window: usize },
    WorkspaceChanged(usize),
    KeyboardLayoutChanged(String),
}

impl Compositor {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Niri(_) => "niri",
            Self::Hyprland(_) => "hyprland",
        }
    }

    pub async fn listen(self, sender: mpsc::Sender<CompositorEvent>) -> anyhow::Result<()> {
        match self {
            Self::Niri(compositor) => compositor.listen(sender).await,
            Self::Hyprland(compositor) => compositor.listen(sender).await,
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
