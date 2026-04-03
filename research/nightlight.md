# Nightlight Provider

**Source:** wlr-gamma-control-unstable-v1 Wayland protocol (direct), GeoClue2 D-Bus for geolocation

**What it does:** The daemon directly controls display color temperature via the Wayland gamma control protocol. It handles sunrise/sunset calculation, smooth transitions, scheduling, and geolocation internally — no external tools (gammastep, wlsunset, etc.) needed.

## System Interface

### wlr-gamma-control-unstable-v1 (Wayland protocol)

The daemon connects to the Wayland display and uses this protocol to set per-output gamma ramps.

Interfaces:
- `zwlr_gamma_control_manager_v1` — factory: creates per-output gamma controllers
- `zwlr_gamma_control_v1` — per-output controller

Flow:
1. Bind to `zwlr_gamma_control_manager_v1` global
2. For each output, call `get_gamma_control(output)` → returns controller + `gamma_size` event
3. Compute gamma ramp from target temperature (Kelvin → RGB using Planckian locus approximation)
4. Write 3 × `gamma_size` × 2 bytes (R, G, B ramps as u16 arrays) to an mmap'd fd
5. Call `set_gamma(fd)` on the controller
6. Listen for `failed` signal (another client took control)

Constraints:
- Exclusive access: only one gamma controller per output
- On disconnect: compositor restores original gamma
- Ramp size is per-output (compositor decides)

Supported by: Sway, Hyprland, Niri, River, LabWC, COSMIC, Wayfire. Not supported by GNOME/KDE.

### Kelvin to RGB conversion

Standard approach: use Planckian locus polynomial approximation (same as used by gammastep/redshift). For a given temperature T in Kelvin (1700–10000):

- Compute R, G, B multipliers (0.0–1.0) from temperature
- Apply multipliers to a linear gamma ramp: `ramp[i] = (i / gamma_size) * multiplier * 65535`
- Optionally apply brightness multiplier before writing

### GeoClue2 (geolocation for auto-schedule)

Service: `org.freedesktop.GeoClue2` (session bus)

Manager (`/org/freedesktop/GeoClue2/Manager`):
- `GetClient() -> ObjectPath`

Client (`/org/freedesktop/GeoClue2/Client/{N}`):
- `Start()` — begin location updates
- `Stop()` — stop updates
- Properties: `Location: ObjectPath`, `Active: bool`
- Signals: `LocationUpdated(old_path: ObjectPath, new_path: ObjectPath)`

Location (`/org/freedesktop/GeoClue2/Location/{N}`):
- `Latitude: f64` — degrees (negative = South)
- `Longitude: f64` — degrees (negative = West)
- `Accuracy: f64` — meters
- `Timestamp: i64` — Unix timestamp

### Sunrise/sunset calculation

The daemon computes sunrise/sunset times internally from latitude, longitude, and current date using standard solar position equations (e.g. NOAA solar calculator algorithm). No external tool or service needed.

Inputs: latitude, longitude, date
Outputs: sunrise time, sunset time (as UTC timestamps or local time)

Transition periods:
- Dawn: sunrise − transition_duration → sunrise
- Dusk: sunset → sunset + transition_duration
- During transition: linearly interpolate temperature between day and night values

## Topics

- `nightlight.status` — current state: enabled, active, temperature, mode, schedule

## Methods

- `nightlight.set_enabled(enabled: bool)` — enable/disable night light
- `nightlight.set_temperature(kelvin: u32)` — set night color temperature (1700–10000)
- `nightlight.set_day_temperature(kelvin: u32)` — set day color temperature (default 6500)
- `nightlight.set_mode(mode: NightlightMode)` — auto, manual schedule, or always-on
- `nightlight.set_schedule(from_hour: f64, to_hour: f64)` — manual schedule (decimal hours 0.0–24.0)
- `nightlight.set_location(latitude: f64, longitude: f64)` — override location for sunrise/sunset
- `nightlight.set_transition_duration(seconds: u32)` — how long the temperature fades (default 1800 = 30min)
- `nightlight.disable_for(minutes: u32)` — temporarily disable for N minutes
- `nightlight.preview(duration_secs: u32, kelvin: u32)` — preview a temperature for N seconds then revert

## Types

```rust
/// How night light schedule is determined
enum NightlightMode {
    /// Disabled entirely
    Off,
    /// Automatic based on sunrise/sunset from geolocation
    Auto,
    /// Manual fixed schedule
    Manual,
    /// Always on at set temperature
    AlwaysOn,
}

/// Current night light state, emitted on `nightlight.status`
struct NightlightStatus {
    /// Whether night light is currently active (gamma is shifted)
    active: bool,
    /// Whether the feature is enabled (may not be active outside schedule)
    enabled: bool,
    /// Current effective color temperature in Kelvin (interpolated during transition)
    temperature: u32,
    /// Target night temperature
    night_temperature: u32,
    /// Target day temperature
    day_temperature: u32,
    mode: NightlightMode,
    /// Schedule start time as decimal hours (e.g. 20.0 = 8 PM)
    schedule_from: Option<f64>,
    /// Schedule end time as decimal hours (e.g. 6.5 = 6:30 AM)
    schedule_to: Option<f64>,
    /// Transition duration in seconds
    transition_duration: u32,
    /// Computed sunrise time (UTC) for today, if in auto mode
    sunrise: Option<String>,
    /// Computed sunset time (UTC) for today, if in auto mode
    sunset: Option<String>,
    /// Location used for auto mode
    latitude: Option<f64>,
    longitude: Option<f64>,
    /// If temporarily disabled, when it re-enables (Unix timestamp)
    disabled_until: Option<u64>,
}
```

## Icons

- `night-light-symbolic` — night light enabled/active
- `night-light-disabled-symbolic` — night light disabled
- `weather-clear-symbolic` — daytime / sun
- `weather-clear-night-symbolic` — nighttime / moon

All icons above are available in Adwaita icon theme.

## Crates

- `wayland-client` — core Wayland client connection
- `wayland-protocols-wlr` (0.3) — wlr-gamma-control-unstable-v1 protocol bindings
- `zbus` (5) — D-Bus client for GeoClue2
- `sunrise-sunset-calculator` — NOAA solar position algorithm for sunrise/sunset from lat/lon
- `chrono` — time calculations for schedule management

## Change Detection

**Internal state:** The daemon owns all state. Changes come from:
- Client method calls (set_enabled, set_temperature, etc.)
- Timer-driven transitions (dawn/dusk interpolation)
- GeoClue2 `LocationUpdated` signal → triggers sunrise/sunset recalculation

No external change detection needed — the daemon is the single source of truth for gamma control.

**Gamma control lost:** If another program (e.g. user runs gammastep manually) takes over wlr-gamma-control, the compositor sends a `failed` signal. The daemon should detect this, mark nightlight as unavailable, and notify subscribers.

## Features

- Direct Wayland gamma control via wlr-gamma-control protocol
- Built-in Kelvin → RGB gamma ramp computation
- Built-in sunrise/sunset calculation from lat/lon (NOAA solar algorithm)
- Automatic geolocation via GeoClue2 D-Bus
- Manual location override
- Smooth transitions with configurable duration (linear interpolation over dawn/dusk)
- Manual schedule with configurable start/end times
- Always-on mode at fixed temperature
- Day and night temperature independently configurable
- "Disable for N minutes" temporary override
- Preview mode (show a temperature for N seconds then revert)
- Per-output gamma control (all monitors or specific ones)
- Graceful handling when gamma control is taken by another client
- Brightness adjustment alongside color temperature

## Notes

- The daemon needs a Wayland connection — it must connect to `$WAYLAND_DISPLAY` to use the gamma protocol
- This is separate from the Unix socket IPC for clients — the daemon is both a Wayland client (for gamma) and a socket server (for panel/CLI clients)
- GNOME and KDE don't support wlr-gamma-control — nightlight won't work on those DEs (they have their own built-in implementations)
- GeoClue2 requires user consent on first use
- Only one gamma controller per output — if the daemon holds it, gammastep/wlsunset cannot run simultaneously (and vice versa)
- Sunrise/sunset calculation needs only basic trig — no external crate required, ~50 lines of code
