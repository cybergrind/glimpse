#![allow(dead_code)]

pub mod compositors;
pub mod hyprland;
pub mod keyboard;
pub mod niri;

pub use compositors::{
    Compositor, CompositorCapabilities, CompositorEvent, CompositorRefresh, CompositorSnapshot,
    CompositorStructureSnapshot, CompositorType, KeyboardLayout, KeyboardLayoutSnapshot, Monitor,
    ScreencastControlCapability, ScreencastKind, ScreencastSession, ScreencastStateCapability,
    ScreencastTarget, Window, Workspace, detect_compositor,
};
pub use keyboard::layout_code as keyboard_layout_code;
