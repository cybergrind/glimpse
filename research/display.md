# Display Provider

**Source:** Compositor IPC (niri, hyprland), DRM/KMS sysfs, EDID, wlr-output-management protocol

**What it does:** Lists connected displays/monitors with their properties (make, model, resolution, refresh rate, scale, position, VRR, DPMS state), and provides display configuration and power management.

## System Interface

### Compositor IPC

#### Niri (`niri msg outputs --json`)

Returns JSON array of Output objects:

```json
{
  "name": "DP-1",
  "make": "Dell",
  "model": "U2723DE",
  "serial": "ABC123XYZ",
  "physical_size": [597, 336],
  "modes": [
    { "width": 2560, "height": 1440, "refresh_rate": 60000, "is_preferred": true },
    { "width": 1920, "height": 1080, "refresh_rate": 60000, "is_preferred": false }
  ],
  "current_mode": 0,
  "is_custom_mode": false,
  "vrr_supported": true,
  "vrr_enabled": false,
  "logical": {
    "x": 0,
    "y": 0,
    "width": 2560,
    "height": 1440,
    "scale": 1.0,
    "transform": "Normal"
  }
}
```

Fields:
- `name: String` — connector name (e.g. "eDP-1", "HDMI-A-1")
- `make: String` — manufacturer name
- `model: String` — model description
- `serial: Option<String>` — serial number (nullable)
- `physical_size: Option<[u32, u32]>` — [width_mm, height_mm] (nullable)
- `modes: Vec<Mode>` — all available display modes
- `current_mode: Option<usize>` — index into `modes` array (null if disabled)
- `is_custom_mode: bool` — whether current mode is custom
- `vrr_supported: bool`
- `vrr_enabled: bool`
- `logical: Option<LogicalOutput>` — null if output is unmapped/disabled

Mode sub-object:
- `width: u16`, `height: u16` — physical pixels
- `refresh_rate: u32` — millihertz (60000 = 60Hz)
- `is_preferred: bool`

LogicalOutput sub-object:
- `x: i32`, `y: i32` — position in compositor space
- `width: u32`, `height: u32` — logical pixels
- `scale: f64`
- `transform: String` — "Normal", "90", "180", "270", "Flipped", "Flipped90", "Flipped180", "Flipped270"

Socket: `$NIRI_SOCKET` or `$XDG_RUNTIME_DIR/niri/{instance}.sock`

Note: JSON output is stable. New fields may be added but existing fields won't be renamed or removed.

#### Hyprland (`hyprctl monitors -j`)

Returns JSON array of monitor objects:

```json
{
  "id": 0,
  "name": "eDP-1",
  "description": "LG Display LG Display 0x0000 (eDP-1)",
  "make": "LG Display",
  "model": "LG Display",
  "serial": "0x00000000",
  "width": 1920,
  "height": 1080,
  "physicalWidth": 276,
  "physicalHeight": 156,
  "refreshRate": 60.0,
  "x": 0,
  "y": 0,
  "scale": 1.0,
  "transform": 0,
  "focused": true,
  "dpmsStatus": true,
  "vrr": 0,
  "activelyTearing": false,
  "disabled": false,
  "currentFormat": "XRGB8888",
  "availableModes": ["1920x1080@60.00Hz", "1680x1050@60.00Hz"],
  "activeWorkspace": { "id": 1, "name": "1" },
  "specialWorkspace": { "id": -99, "name": "special" },
  "reserved": [0, 0, 0, 0],
  "mirrorOf": "none",
  "solitary": "0x0",
  "colorManagementPreset": "",
  "sdrBrightness": 1.0,
  "sdrSaturation": 1.0
}
```

Fields:
- `id: u32` — monitor ID
- `name: String` — connector name
- `description: String` — full description with make/model/serial
- `make: String` — manufacturer
- `model: String` — model name
- `serial: String` — always present (may be empty or hex)
- `width: u32`, `height: u32` — current resolution in pixels
- `physicalWidth: u32`, `physicalHeight: u32` — physical size in mm
- `refreshRate: f64` — refresh rate in Hz (e.g. 60.0, 144.05)
- `x: i32`, `y: i32` — position
- `scale: f64` — scaling factor
- `transform: u32` — rotation (0=normal, 1=90, 2=180, 3=270, 4-7=flipped variants)
- `focused: bool` — whether currently focused
- `dpmsStatus: bool` — display power on/off
- `vrr: u32` — variable refresh rate (0=disabled, 1=enabled)
- `activelyTearing: bool`
- `disabled: bool`
- `currentFormat: String` — pixel format (e.g. "XRGB8888")
- `availableModes: Vec<String>` — format: "WIDTHxHEIGHT@REFRESH.00Hz"
- `activeWorkspace: { id: i32, name: String }` — always present
- `specialWorkspace: { id: i32, name: String }` — always present
- `reserved: [u32; 4]` — reserved space [top, right, bottom, left]
- `mirrorOf: String` — "none" or mirrored monitor name
- `colorManagementPreset: String`
- `sdrBrightness: f64`, `sdrSaturation: f64`

Command socket: `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`
Event socket: `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`

### DRM/KMS sysfs (fallback)

Path: `/sys/class/drm/card{N}-{connector}/`

Files:
- `status` — "connected", "disconnected", or "unknown"
- `edid` — binary EDID blob (128+ bytes)
- `modes` — newline-separated list of supported modes (e.g. "1920x1080 60")
- `enabled` — "enabled" or "disabled"
- `dpms` — "On", "Off", "Standby", "Suspend"

### EDID (Extended Display Identification Data)

Read from `/sys/class/drm/card{N}-{connector}/edid` (binary).

Contains:
- Manufacturer ID (3-letter PNP code, bytes 8-9, 5-bit compressed ASCII)
- Product code (bytes 10-11, little-endian)
- Serial number (bytes 12-15)
- Manufacturing week/year (bytes 16-17, year = value + 1990)
- Physical dimensions in mm
- Supported resolutions and timings
- Display name (descriptor block)
- Color depth and color space info

### wlr-output-power-management (DPMS on Wayland)

Protocol: `zwlr_output_power_management_unstable_v1`

Per-output power control:
- Mode 0 = Off
- Mode 1 = On

CLI tool: `wlopm --on HDMI-A-1` / `wlopm --off HDMI-A-1`

Supported by: Sway, Hyprland, River, COSMIC, LabWC. Not supported by GNOME/KDE.

## Topics

- `display.outputs` — list of all connected displays with full properties
- `display.output.{name}` — single display state

## Methods

- `display.set_enabled(name: String, enabled: bool)` — enable/disable a display
- `display.set_dpms(name: String, state: DpmsState)` — control display power (on/off)
- `display.set_mode(name: String, width: u32, height: u32, refresh_rate_mhz: u32)` — set resolution and refresh rate
- `display.set_scale(name: String, scale: f64)` — set scaling factor
- `display.set_position(name: String, x: i32, y: i32)` — set display position
- `display.set_transform(name: String, transform: Transform)` — set rotation

## Types

```rust
/// Display rotation/flip transform
enum Transform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    FlippedRotate90,
    FlippedRotate180,
    FlippedRotate270,
}

/// Display power state
enum DpmsState {
    On,
    Off,
}

/// A display mode (resolution + refresh rate)
struct DisplayMode {
    width: u32,
    height: u32,
    /// Refresh rate in millihertz (e.g. 60000 = 60Hz)
    refresh_rate_mhz: u32,
    /// Whether this is the preferred/native mode
    preferred: bool,
}

/// A connected display output
struct DisplayOutput {
    /// Connector name (e.g. "eDP-1", "HDMI-A-1")
    name: String,
    /// Manufacturer name or PNP ID
    make: String,
    /// Model name from EDID
    model: String,
    /// Serial number (may be empty)
    serial: String,
    /// Physical width in millimeters
    physical_width_mm: u32,
    /// Physical height in millimeters
    physical_height_mm: u32,
    /// Whether the display is enabled
    enabled: bool,
    /// Current mode
    current_mode: Option<DisplayMode>,
    /// All available modes
    available_modes: Vec<DisplayMode>,
    /// Position in global compositor space
    x: i32,
    y: i32,
    /// Scaling factor
    scale: f64,
    transform: Transform,
    dpms: DpmsState,
    /// Variable refresh rate enabled
    vrr: bool,
    /// Whether this is the focused/primary display
    focused: bool,
}

/// Emitted on `display.outputs`
struct DisplayOutputList {
    outputs: Vec<DisplayOutput>,
}
```

## Icons

- `video-display-symbolic` — generic display/monitor
- `computer-symbolic` — fallback for display device
- `preferences-desktop-display-symbolic` — display settings

All icons above are available in Adwaita icon theme.

## Crates

- `niri-ipc` — Niri compositor IPC bindings (typed Rust structs, serde)
- `hyprland` (0.4) — Hyprland IPC wrapper (async, typed)
- `wayland-client` — core Wayland client for wlr-output-management protocol
- `wayland-protocols-wlr` (0.3) — wlr protocol bindings (output management, power management)
- `edid-rs` — pure-Rust EDID parsing (no_std compatible)
- `inotify` — watch /sys/class/drm status changes (udev fallback)

## Change Detection

**Hyprland event socket:** Line-delimited events on `.socket2.sock`:
- `monitoradded>>MONITORNAME` — monitor connected
- `monitorremoved>>MONITORNAME` — monitor disconnected
- `monitoraddedv2>>WORKSPACE_ID,MONITOR_NAME,DESCRIPTION` — v2 with more context
- `monitorremovedv2>>MONITOR_ID,MONITOR_NAME,DESCRIPTION` — v2 with more context
- `focusedmon>>MONITORNAME,WORKSPACENAME` — focus changed
- `focusedmonv2>>ID,MONITORNAME` — v2 focus changed

**Sway IPC:** Subscribe to `output` events:
- `{"change":"connected","output":{...}}`
- `{"change":"disconnected","output":{...}}`
- `{"change":"dpms","output":{...}}`

**Niri:** No output change event subscription in IPC yet. Requires polling `niri msg outputs`.

**udev events (universal fallback):** Listen for `change` events on `subsystem=drm` with `HOTPLUG=1`. Fires when monitors are physically connected/disconnected. Works regardless of compositor.

**DRM sysfs polling (last resort):** Poll `/sys/class/drm/card*-*/status` files.

**Detection pipeline:** Physical hotplug → kernel detects → udev event → connector status updated in sysfs → new EDID available → compositor broadcasts IPC event.

## Features

- List connected displays with make, model, serial, physical dimensions
- Current and available display modes (resolution + refresh rate)
- Display position in compositor coordinate space
- Fractional scaling support
- Transform/rotation control
- DPMS power management (on/off)
- Variable refresh rate (VRR) status
- Display enable/disable
- EDID parsing for manufacturer and model identification
- Monitor hotplug detection (connect/disconnect)
- Resolution and refresh rate switching
- Display arrangement/positioning
- Mirror mode detection
- HDR/SDR brightness info (Hyprland)
- Pixel format info

## Notes

- Compositor IPC is the primary source — DRM/KMS sysfs is fallback only
- EDID parsing is needed for DRM fallback path; compositor IPC usually provides make/model directly
- wlr-output-management is the Wayland-native way to configure displays but requires a Wayland client connection, not just socket IPC
- GNOME and KDE use their own D-Bus interfaces — not covered here as we target wlroots-based compositors
- DDC/CI brightness control is in the brightness provider, not here
