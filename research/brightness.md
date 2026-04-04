# Brightness Provider

**Source:** /sys/class/backlight (internal), ddcutil CLI (external monitors), iio-sensor-proxy D-Bus (ambient light)

**What it does:** Controls display brightness for internal and external monitors, reads ambient light sensor data for automatic brightness adjustment.

## System Interface

### /sys/class/backlight/{device}/ (internal displays)

Files:
- `brightness` (R/W) — current brightness value (0 to max_brightness)
- `actual_brightness` (RO) — actual hardware brightness (may differ from `brightness`)
- `max_brightness` (RO) — maximum brightness value
- `type` (RO) — control method: "firmware", "platform", or "raw"
- `bl_power` (R/W) — power state: 0=on (FB_BLANK_UNBLANK), 4=off (FB_BLANK_POWERDOWN)

Backlight type priority (prefer first available):
1. `firmware` — standard firmware interface, least conflicts
2. `platform` — vendor/hardware-specific
3. `raw` — direct hardware register, last resort

Permissions: writable by root only by default. Access via udev rules (video/input group) or systemd-logind API.

### ddcutil CLI (external monitors via DDC/CI over I2C)

Commands:
- `ddcutil detect` — list connected DDC-capable monitors
- `ddcutil getvcp 10` — get brightness; output: `VCP code 0x10 (Brightness): current value = 57, max value = 100`
- `ddcutil getvcp 10 --terse` — machine-readable; output: `10 c 57 100` (code, type, current, max)
- `ddcutil setvcp 10 75` — set brightness to 75
- `ddcutil getvcp 12` — get contrast (VCP 0x12)
- `ddcutil setvcp 12 50` — set contrast

VCP codes:
- `0x10` — Brightness (continuous, 0–max)
- `0x12` — Contrast (continuous, 0–max)

Performance: ~90% of time spent in DDC/CI-mandated waits (30ms minimum between packets). Use `--sleep-multiplier 0.2` to reduce waits to 20% of spec.

Caveats:
- Not all monitors support DDC/CI
- Dynamic Contrast Range (DCR) may disable VCP 0x10
- Some monitors auto-revert brightness after ~1 second
- I2C bus conflicts with multiple monitors

### net.hadess.SensorProxy (object: `/net/hadess/SensorProxy`)

Methods:
- `ClaimLight()` — start receiving ambient light updates
- `ReleaseLight()` — stop receiving updates (saves battery)

Properties:
- `HasAmbientLight: bool` — whether ALS hardware is present
- `LightLevel: f64` — current ambient light reading
- `LightLevelUnit: String` — "lux" or "vendor"

Signals:
- `PropertiesChanged` (via `org.freedesktop.DBus.Properties`)

## Topics

- `brightness.displays` — list of controllable displays with current brightness
- `brightness.display.{id}` — single display brightness state
- `brightness.ambient` — ambient light sensor reading

## Methods

- `brightness.set(display_id: String, value: u32)` — set brightness to absolute value (0 to display max)
- `brightness.set_relative(display_id: String, delta: i32, is_percentage: bool)` — adjust brightness by delta; positive = brighter, negative = dimmer

## Types

```rust
/// How the display brightness is controlled
enum BacklightType {
    /// Standard firmware interface (preferred)
    Firmware,
    /// Platform/vendor-specific
    Platform,
    /// Direct hardware register (last resort)
    Raw,
    /// External monitor via DDC/CI
    DdcCi,
}

/// A controllable display
struct BrightnessDisplay {
    /// Unique identifier (sysfs device name or DDC bus id)
    id: String,
    /// Human-readable name
    name: String,
    backlight_type: BacklightType,
    /// Current brightness value
    current: u32,
    /// Maximum brightness value
    max: u32,
    /// Current brightness as percentage 0.0–100.0
    percentage: f64,
    /// Whether display is powered on
    powered: bool,
}

/// Emitted on `brightness.displays`
struct BrightnessDisplayList {
    displays: Vec<BrightnessDisplay>,
}

/// Ambient light sensor state, emitted on `brightness.ambient`
struct AmbientLight {
    available: bool,
    /// Light level reading
    level: f64,
    /// Unit: "lux" or "vendor"
    unit: String,
}

/// Request to set brightness
struct SetBrightnessRequest {
    display_id: String,
    /// Brightness value (0 to display max)
    value: u32,
}

/// Request to adjust brightness relatively
struct SetBrightnessRelativeRequest {
    display_id: String,
    /// Delta to apply (positive = brighter, negative = dimmer)
    delta: i32,
    /// Whether delta is a percentage rather than absolute
    is_percentage: bool,
}
```

## Icons

- `display-brightness-symbolic` — generic brightness control
- `display-brightness-high-symbolic` — high brightness
- `display-brightness-low-symbolic` — low brightness
- `display-brightness-off-symbolic` — display off / zero brightness

All icons above are available in Adwaita icon theme.

## Crates

- `inotify` — watch /sys/class/backlight file changes
- `nix` — sysfs file read/write, permissions
- `ddcutil` (0.0.3) — Rust bindings for libddcutil (DDC/CI); alternatively shell out to `ddcutil` CLI
- `zbus` (5) — D-Bus client for iio-sensor-proxy (ambient light)

## Change Detection

**Internal backlight:** inotify watch on `/sys/class/backlight/{device}/actual_brightness` — fires when any program or hardware key changes brightness. Catches brightnessctl, GNOME settings, keyboard brightness keys, everything.

**External monitors (DDC/CI):** No change notification exists. DDC/CI is poll-only — the monitor cannot notify the host of changes. Options: periodic polling (every 5–10s), or only track our own writes and accept staleness.

**Ambient light sensor:** `PropertiesChanged` D-Bus signal from iio-sensor-proxy. Fully reactive.

## Features

- Internal backlight control via /sys/class/backlight
- External monitor brightness via ddcutil (DDC/CI, VCP code 0x10)
- Ambient light sensor integration (iio-sensor-proxy)
- Per-display brightness tracking and control
- Absolute and relative brightness adjustment
- Display power state control (on/off via bl_power)
- Backlight type detection and priority (firmware > platform > raw)
- Smooth brightness transitions (step-by-step to avoid flicker)
- Minimum brightness floor to prevent black screen
- Per-display brightness profiles
- Contrast control for external monitors (VCP 0x12)
- Auto-brightness based on ambient light sensor

## Notes

- brightnessctl can be used as a fallback CLI tool instead of raw sysfs access
- DDC/CI is slow (~30-100ms per operation) — cache values and debounce writes
- Always release ambient light sensor claim when not needed (battery impact)
- Multiple backlight interfaces may exist for one display — prefer firmware > platform > raw
- Write permissions to sysfs require either udev rules or logind integration
