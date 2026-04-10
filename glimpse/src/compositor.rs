//! Compositor detection utilities.
//!
//! Detects the running Wayland compositor by inspecting environment variables
//! set by each compositor on session start.

/// The Wayland compositor currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compositor {
    /// [Niri](https://github.com/YaLTeR/niri) — scrollable-tiling Wayland compositor.
    Niri,
    /// [Hyprland](https://hyprland.org/) — dynamic tiling Wayland compositor.
    Hyprland,
    /// Any other or unknown compositor.
    Other,
}

impl Compositor {
    /// Returns `true` if running under Niri.
    pub fn is_niri(self) -> bool {
        self == Self::Niri
    }

    /// Returns `true` if running under Hyprland.
    pub fn is_hyprland(self) -> bool {
        self == Self::Hyprland
    }
}

/// Detect the active Wayland compositor.
///
/// Detection order:
/// 1. `HYPRLAND_INSTANCE_SIGNATURE` — set by Hyprland.
/// 2. `NIRI_SOCKET` — set by Niri.
/// 3. Falls back to [`Compositor::Other`].
pub fn detect() -> Compositor {
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        Compositor::Hyprland
    } else if std::env::var("NIRI_SOCKET").is_ok() {
        Compositor::Niri
    } else {
        Compositor::Other
    }
}

/// Returns `true` if the current compositor supports a given feature.
///
/// Useful for conditionally enabling compositor-specific functionality
/// (workspace watching, IPC, etc.) without hard-coding compositor checks
/// throughout the codebase.
pub fn supports_niri_ipc() -> bool {
    detect().is_niri()
}

pub fn supports_hyprland_ipc() -> bool {
    detect().is_hyprland()
}
