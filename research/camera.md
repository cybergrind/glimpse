# Camera Provider

**Source:** PipeWire for camera node enumeration, /sys/class/video4linux for device info

**What it does:** Lists camera devices, reports their state (in use / idle), and provides a privacy toggle.

## System Interface

### PipeWire (camera nodes)

PipeWire manages camera devices as nodes with `media.class = "Video/Source"`.

`pw-cli list-objects` — lists all PipeWire objects including camera nodes.

`pw-dump` — JSON dump of all nodes. Camera nodes have:
```json
{
  "type": "PipeWire:Interface:Node",
  "info": {
    "props": {
      "media.class": "Video/Source",
      "node.name": "v4l2_input.pci-0000_00_14.0-usb-0_8_1.0",
      "node.description": "HD Webcam",
      "device.api": "v4l2",
      "device.path": "/dev/video0"
    },
    "state": "idle"
  }
}
```

Node states: "error", "creating", "suspended", "idle", "running"
- `running` = camera is actively being used by an application
- `idle` = camera available but not in use

### /sys/class/video4linux/video{N}/

Files:
- `name` — device name
- `dev` — major:minor device numbers
- `index` — device index

Also: `/dev/video{N}` device files.

### Hardware kill switches

Some laptops have hardware camera kill switches:
- Shows up in `/sys/class/leds/` as camera LED controls
- Or via rfkill-like mechanism (rare for cameras)
- No standard interface — hardware-specific

## Topics

- `camera.devices` — list of camera devices with state
- `camera.device.{id}` — single camera state

## Methods

- `camera.set_enabled(device_id: String, enabled: bool)` — software disable/enable a camera (via PipeWire node suspend)

## Types

```rust
/// Camera device state
enum CameraState {
    /// Available but not in use
    Idle,
    /// Actively streaming to an application
    Running,
    /// Suspended/disabled
    Suspended,
    /// Error state
    Error,
}

/// A camera device
struct CameraDevice {
    /// PipeWire node ID or device path
    id: String,
    /// Human-readable name
    name: String,
    /// Device path (e.g. "/dev/video0")
    device_path: String,
    state: CameraState,
    /// Whether camera is currently in use by any application
    in_use: bool,
}
```

## Icons

- `camera-web-symbolic` — webcam
- `camera-disabled-symbolic` — camera off/blocked

All icons above are available in Adwaita icon theme.

## Crates

- `pipewire` (0.9) — PipeWire Rust bindings for node enumeration and state monitoring

## Change Detection

**PipeWire node state changes:** Monitor node state transitions (idle → running = camera started, running → idle = camera stopped). Use PipeWire's event loop or `pw-mon` approach.

## Features

- List camera devices with names and paths
- Report camera in-use state (which app is using it)
- Software camera disable/enable
- Hardware kill switch detection (when available)
- Camera state change events (started/stopped streaming)

## Notes

- PipeWire is required for camera node monitoring — on systems without PipeWire, fall back to V4L2 device enumeration only
- "Running" state means an app has an active stream — useful for privacy indicators
- Software disable works by suspending the PipeWire node — apps see the camera as unavailable
- Hardware kill switches are hardware-specific and not reliably detectable
- This provider feeds into the privacy provider for camera-in-use indicators
