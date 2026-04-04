# Accessibility Provider

**Source:** gsettings (`org.gnome.desktop.interface`, `org.gnome.desktop.a11y.*`), XDG portal

**What it does:** Reports and controls accessibility settings — font scaling, high contrast, screen reader, reduce motion, keyboard accessibility features.

## System Interface

### gsettings schemas

#### org.gnome.desktop.interface
- `text-scaling-factor: f64` (RW) — font scaling (1.0 = normal, 1.5 = 150%)
- `cursor-size: u32` (RW) — cursor size in pixels (24 = default)
- `font-name: String` (RW) �� system font

#### org.gnome.desktop.a11y
- `always-show-universal-access-status: bool` (RW)

#### org.gnome.desktop.a11y.interface
- `high-contrast: bool` (RW) — high contrast mode

#### org.gnome.desktop.a11y.keyboard
- `stickykeys-enable: bool` (RW) — sticky keys (modifier keys latch)
- `slowkeys-enable: bool` (RW) — slow keys (hold key before accepting)
- `bouncekeys-enable: bool` (RW) — bounce keys (ignore rapid repeated keystrokes)
- `mousekeys-enable: bool` (RW) — move cursor with keyboard numpad
- `togglekeys-enable: bool` (RW) — audio feedback for Caps/Num/Scroll Lock

#### org.gnome.desktop.a11y.magnifier
- `mag-factor: f64` (RW) — magnification level
- `screen-position: String` (RW) — "full-screen", "top-half", "bottom-half", "left-half", "right-half"

#### org.gnome.desktop.a11y.applications
- `screen-reader-enabled: bool` (RW)
- `screen-magnifier-enabled: bool` (RW)
- `screen-keyboard-enabled: bool` (RW)

### XDG Portal (limited)

`org.freedesktop.portal.Settings` → `org.freedesktop.appearance`:
- `color-scheme` — dark/light (covered in theme provider)

No portal-specific accessibility settings beyond appearance.

## Topics

- `accessibility.settings` — all accessibility settings

## Methods

- `accessibility.set_text_scaling(factor: f64)` — set font scaling (via gsettings)
- `accessibility.set_cursor_size(size: u32)` — set cursor size
- `accessibility.set_high_contrast(enabled: bool)` — toggle high contrast
- `accessibility.set_screen_reader(enabled: bool)` — toggle screen reader
- `accessibility.set_screen_magnifier(enabled: bool, factor: Option<f64>)` — toggle magnifier
- `accessibility.set_screen_keyboard(enabled: bool)` — toggle on-screen keyboard
- `accessibility.set_sticky_keys(enabled: bool)` — toggle sticky keys
- `accessibility.set_slow_keys(enabled: bool)` — toggle slow keys
- `accessibility.set_bounce_keys(enabled: bool)` �� toggle bounce keys
- `accessibility.set_reduce_motion(enabled: bool)` — toggle reduce animations

## Types

```rust
/// All accessibility settings, emitted on `accessibility.settings`
struct AccessibilitySettings {
    /// Font scaling factor (1.0 = normal)
    text_scaling: f64,
    /// Cursor size in pixels
    cursor_size: u32,
    /// High contrast mode
    high_contrast: bool,
    /// Screen reader active
    screen_reader: bool,
    /// Screen magnifier active
    screen_magnifier: bool,
    /// Magnification factor
    mag_factor: f64,
    /// On-screen keyboard active
    screen_keyboard: bool,
    /// Sticky keys (modifiers latch on single press)
    sticky_keys: bool,
    /// Slow keys (must hold key before accepting)
    slow_keys: bool,
    /// Bounce keys (ignore rapid repeated presses)
    bounce_keys: bool,
    /// Mouse keys (numpad controls cursor)
    mouse_keys: bool,
    /// Reduce motion/animations
    reduce_motion: bool,
}
```

## Icons

- `preferences-desktop-accessibility-symbolic` — accessibility settings

## Crates

- `zbus` (5) — for gsettings via dconf D-Bus (or shell out to `gsettings` CLI)

## Change Detection

**gsettings/dconf:** `PropertiesChanged` on dconf, or portal `SettingChanged` signal for cross-DE settings.

## Features

- Font scaling control
- Cursor size control
- High contrast mode toggle
- Screen reader toggle (Orca)
- Screen magnifier with zoom level
- On-screen keyboard toggle
- Sticky keys, slow keys, bounce keys
- Mouse keys (numpad cursor control)
- Reduce motion preference

## Notes

- Most settings are GNOME-specific (gsettings) — may not work on KDE or other DEs
- Screen reader control starts/stops Orca — requires Orca installed
- `reduce_motion` maps to `org.gnome.desktop.interface.enable-animations` (inverted)
- For non-GNOME DEs, accessibility settings vary significantly — this provider is GNOME-focused
