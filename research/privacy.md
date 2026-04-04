# Privacy Provider

**Source:** Aggregates state from other providers (camera, audio, screen_capture) + PipeWire node monitoring

**What it does:** Reports which applications are currently accessing the microphone, camera, or sharing the screen. Provides global mic/camera mute.

## System Interface

This is a **meta-provider** — it doesn't talk to a system service directly. Instead it aggregates state from:

### Microphone in use
- PipeWire: nodes with `media.class = "Stream/Input/Audio"` in `running` state indicate active recording
- PulseAudio: active `source-output` streams indicate recording apps
- Source: audio provider data

### Camera in use
- PipeWire: nodes with `media.class = "Video/Source"` in `running` state
- Source: camera provider data

### Screen sharing active
- XDG Desktop Portal: active ScreenCast sessions
- Source: screen_capture provider data

### Application identification
- PipeWire nodes have `application.name` and `application.icon-name` properties
- PulseAudio streams have `application.name` in properties
- Portal sessions have the requesting app's info

## Topics

- `privacy.indicators` — which privacy-sensitive resources are currently in use, by which apps

## Methods

- `privacy.mute_microphone(muted: bool)` — global mic mute (delegates to audio provider)
- `privacy.disable_camera(disabled: bool)` — global camera disable (delegates to camera provider)

## Types

```rust
/// Type of privacy-sensitive resource
enum PrivacyResourceType {
    Microphone,
    Camera,
    ScreenShare,
    Location,
}

/// An app accessing a privacy-sensitive resource
struct PrivacyAccess {
    resource: PrivacyResourceType,
    /// Application name
    app_name: String,
    /// Application icon (freedesktop icon name)
    app_icon: Option<String>,
    /// PID of the accessing process
    pid: Option<u32>,
}

/// Privacy indicators, emitted on `privacy.indicators`
struct PrivacyIndicators {
    /// Currently active accesses
    accesses: Vec<PrivacyAccess>,
    /// Whether microphone is globally muted
    mic_muted: bool,
    /// Whether camera is globally disabled
    camera_disabled: bool,
    /// Summary flags
    mic_in_use: bool,
    camera_in_use: bool,
    screen_sharing: bool,
    location_in_use: bool,
}
```

## Icons

- `microphone-sensitivity-high-symbolic` — mic active
- `microphone-sensitivity-muted-symbolic` — mic muted
- `camera-web-symbolic` — camera active
- `camera-disabled-symbolic` — camera disabled
- `screen-shared-symbolic` — screen sharing (if available)

All icons above are available in Adwaita icon theme.

## Crates

No additional crates — delegates to audio, camera, and screen_capture providers.

## Change Detection

**Reactive from source providers:**
- Audio provider: stream start/stop events → mic in use
- Camera provider: node state changes → camera in use
- Screen capture provider: session start/stop → screen sharing

The privacy provider subscribes to these internal provider events and aggregates them.

## Features

- Real-time microphone-in-use indicator with app name
- Real-time camera-in-use indicator with app name
- Screen sharing indicator
- Location access indicator (from geolocation provider)
- Per-app resource access listing
- Global microphone mute
- Global camera disable
- Process identification (PID) for accessing apps

## Notes

- This is a meta-provider that aggregates from audio, camera, screen_capture, and geolocation providers
- Privacy indicators should be high-priority, low-latency events — users need to know immediately
- Global mic mute sets the default source volume to 0 or mute — not all apps respect this
- Camera disable suspends the PipeWire node — more reliable than mute
- Location access tracking requires geolocation provider to report when GeoClue2 clients are active
