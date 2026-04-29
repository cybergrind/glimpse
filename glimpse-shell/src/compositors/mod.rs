#![allow(dead_code)]

pub mod compositors;
pub mod hyprland;
pub mod niri;

pub use compositors::{
    Compositor, CompositorCapabilities, CompositorEvent, CompositorRefresh, CompositorSnapshot,
    CompositorStructureSnapshot, CompositorType, KeyboardLayout, KeyboardLayoutSnapshot, Monitor,
    Window, Workspace, detect_compositor,
};
