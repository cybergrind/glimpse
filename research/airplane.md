# Airplane Provider

**Source:** rfkill sysfs (`/sys/class/rfkill/`), rfkill CLI

**What it does:** Reports and controls airplane mode — soft-blocking/unblocking radio transmitters (WiFi, Bluetooth, WWAN, etc.) and detecting hardware kill switches.

## System Interface

### /sys/class/rfkill/rfkill{N}/

Each radio device has a directory with:
- `type` (RO) — device type: "wlan", "bluetooth", "uwb", "wimax", "wwan", "gps", "fm", "nfc"
- `name` (RO) — device name
- `state` (RO) — effective state: 0=soft-blocked, 1=unblocked, 2=hard-blocked
- `soft` (RW) — software block: 0=unblocked, 1=blocked (writable by root)
- `hard` (RO) — hardware block: 0=unblocked, 1=blocked (physical switch, read-only)

### rfkill CLI

- `rfkill list` — list all devices with block state
- `rfkill list wifi` — list only WiFi devices
- `rfkill block all` — soft-block all radios (airplane mode on)
- `rfkill unblock all` — unblock all radios (airplane mode off)
- `rfkill block wifi` — block WiFi only
- `rfkill block bluetooth` — block Bluetooth only
- `rfkill block 0` — block specific device by index

JSON output: `rfkill --json list`

```json
{
  "rfkilldevices": [
    { "id": 0, "type": "wlan", "device": "phy0", "soft": "unblocked", "hard": "unblocked" },
    { "id": 1, "type": "bluetooth", "device": "hci0", "soft": "unblocked", "hard": "unblocked" }
  ]
}
```

### /dev/rfkill (event interface)

Character device that can be read for rfkill events. Each event is a `struct rfkill_event` (8 bytes):
- `idx: u32` — device index
- `type: u8` — device type
- `op: u8` — operation: 0=add, 1=del, 2=change, 3=change_all
- `soft: u8` — soft block state
- `hard: u8` — hard block state

Can be monitored via poll/epoll for real-time events without polling sysfs.

## Topics

- `airplane.status` — overall airplane mode state, per-radio states

## Methods

- `airplane.set_enabled(enabled: bool)` — block/unblock all radios
- `airplane.set_radio(radio_type: RadioType, blocked: bool)` — block/unblock specific radio type

## Types

```rust
/// Radio transmitter type
enum RadioType {
    Wlan,
    Bluetooth,
    Wwan,
    Gps,
    Nfc,
    Fm,
    Uwb,
    Wimax,
}

/// Block state of a radio
enum BlockState {
    /// Radio is active
    Unblocked,
    /// Blocked by software (can be unblocked)
    SoftBlocked,
    /// Blocked by hardware switch (cannot be unblocked by software)
    HardBlocked,
}

/// A single rfkill device
struct RfkillDevice {
    index: u32,
    radio_type: RadioType,
    name: String,
    soft_blocked: bool,
    hard_blocked: bool,
}

/// Overall airplane mode status, emitted on `airplane.status`
struct AirplaneStatus {
    /// True if all radios are soft-blocked
    airplane_mode: bool,
    /// Per-device states
    devices: Vec<RfkillDevice>,
    /// Any device hard-blocked by hardware switch
    has_hard_block: bool,
}
```

## Icons

- `airplane-mode-symbolic` — airplane mode on
- `airplane-mode-disabled-symbolic` — airplane mode off (if available)
- `network-wireless-disabled-symbolic` — WiFi blocked
- `bluetooth-disabled-symbolic` — Bluetooth blocked

All icons above are available in Adwaita icon theme.

## Crates

- `nix` — read/write sysfs files, poll `/dev/rfkill`
- `inotify` — watch sysfs state changes (alternative to /dev/rfkill polling)

## Change Detection

**`/dev/rfkill` event device (preferred):** Read rfkill_event structs from `/dev/rfkill` using poll/epoll. Fires on any radio state change (software toggle, hardware switch, device add/remove). Fully reactive.

**sysfs inotify (fallback):** Watch `/sys/class/rfkill/rfkill*/state` files for changes.

**rfkill CLI polling (last resort):** Periodically run `rfkill --json list`.

## Features

- Report airplane mode state (all radios blocked)
- Per-radio type blocking (WiFi, Bluetooth, WWAN independently)
- Hardware kill switch detection (hard-block state)
- Device add/remove detection
- Granular control: block WiFi but keep Bluetooth
- Real-time event monitoring via /dev/rfkill

## Notes

- Writing to sysfs `soft` file requires root or appropriate permissions (typically `rfkill` group)
- Hardware blocks cannot be overridden by software — UI should show this distinctly
- "Airplane mode" is a UI concept — under the hood it's just soft-blocking all radios
- WiFi and Bluetooth have their own enable/disable in NetworkManager and BlueZ respectively — rfkill is the lower-level mechanism
