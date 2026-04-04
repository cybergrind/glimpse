# Theme Provider

**Source:** XDG Desktop Portal D-Bus (`org.freedesktop.portal.Settings`, session bus), gsettings (`org.gnome.desktop.interface`)

**What it does:** Reports current dark/light mode preference, accent color, GTK/icon/cursor theme names, and provides mode switching.

## System Interface

### org.freedesktop.portal.Settings (object: `/org/freedesktop/portal/desktop`)

Interface: `org.freedesktop.portal.Settings`

Methods:
- `ReadOne(namespace: String, key: String) -> Variant` — read a single setting
- `ReadAll(namespaces: Vec<String>) -> HashMap<String, HashMap<String, Variant>>` — read all settings in namespaces

Signals:
- `SettingChanged(namespace: String, key: String, value: Variant)` — fires on any setting change

### org.freedesktop.appearance namespace (via portal)

Keys:
- `color-scheme: u32` — 0=no preference, 1=prefer-dark, 2=prefer-light
- `accent-color: (f64, f64, f64)` — RGB tuple in [0.0, 1.0]; out-of-range means unset

The portal is **read-only** — it reflects the system setting but cannot change it.

### org.gnome.desktop.interface (gsettings)

Schema: `org.gnome.desktop.interface`

Keys (read/write via `gsettings` CLI or dconf D-Bus):
- `color-scheme: String` — "default", "prefer-dark", or "prefer-light"
- `gtk-theme: String` — GTK theme name (e.g. "Adwaita", "Adwaita-dark")
- `icon-theme: String` — icon theme name (e.g. "Adwaita", "Papirus")
- `cursor-theme: String` — cursor theme name
- `cursor-size: u32` — cursor size in pixels
- `font-name: String` — system font (e.g. "Cantarell 11")
- `monospace-font-name: String` — monospace font
- `text-scaling-factor: f64` — font scaling (1.0 = normal)

Setting dark mode:
```bash
gsettings set org.gnome.desktop.interface color-scheme prefer-dark
```

### Detection priority

1. Query portal `org.freedesktop.appearance.color-scheme` (cross-DE, works with Flatpak)
2. Fall back to gsettings `org.gnome.desktop.interface color-scheme` (GNOME-specific)
3. Fall back to checking GTK theme name for "dark" substring

## Topics

- `theme.mode` — current dark/light preference, accent color, theme names

## Methods

- `theme.set_mode(mode: ColorScheme)` — set dark/light preference (writes via gsettings)
- `theme.set_gtk_theme(name: String)` — set GTK theme
- `theme.set_icon_theme(name: String)` — set icon theme
- `theme.set_cursor_theme(name: String)` — set cursor theme
- `theme.set_font(name: String)` — set system font
- `theme.set_text_scaling(factor: f64)` — set font scaling

## Types

```rust
/// Color scheme preference
enum ColorScheme {
    /// No preference / follow system
    Default,
    /// Prefer dark mode
    PreferDark,
    /// Prefer light mode
    PreferLight,
}

/// Current theme state, emitted on `theme.mode`
struct ThemeStatus {
    color_scheme: ColorScheme,
    /// Accent color RGB [0.0–1.0] (None if not set)
    accent_color: Option<(f64, f64, f64)>,
    /// GTK theme name
    gtk_theme: Option<String>,
    /// Icon theme name
    icon_theme: Option<String>,
    /// Cursor theme name
    cursor_theme: Option<String>,
    /// System font
    font: Option<String>,
    /// Font scaling factor
    text_scaling: f64,
}
```

## Icons

- `preferences-desktop-appearance-symbolic` — appearance settings
- `weather-clear-symbolic` — light mode
- `weather-clear-night-symbolic` — dark mode

All icons above are available in Adwaita icon theme.

## Crates

- `zbus` (5) — D-Bus client for portal Settings

## Change Detection

**Portal:** `SettingChanged(namespace, key, value)` signal on `org.freedesktop.portal.Settings`. Fully reactive — fires when any desktop setting changes.

**gsettings:** Monitor via dconf `PropertiesChanged` signals, or watch `SettingChanged` portal signal (GNOME's portal backend emits this when gsettings change).

## Features

- Read current dark/light mode preference
- Read accent color
- Read GTK, icon, cursor theme names
- Read system font and scaling
- Set dark/light mode
- Set GTK, icon, cursor themes
- Set font and text scaling
- Cross-DE detection via portal (works with GNOME, KDE portal backends, niri)
- Fallback to gsettings for GNOME-specific settings
- Scheduled theme switching (based on time or nightlight state)

## Notes

- Portal is read-only — setting changes must go through gsettings/dconf
- Portal `SettingChanged` signal is the most reliable cross-DE notification
- Not all DEs support accent-color — may be unset
- KDE uses its own settings backend — gsettings won't work there
- Theme names can be enumerated from `/usr/share/themes/` (GTK), `/usr/share/icons/` (icons), `/usr/share/icons/*/cursors/` (cursors)
