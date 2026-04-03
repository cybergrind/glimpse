# Audio Provider

**Source:** WirePlumber/PipeWire (wpctl CLI), PulseAudio (pactl CLI) as fallback

**What it does:** Lists audio outputs (sinks), inputs (sources), and streams (apps). Provides per-device and per-stream volume control, mute, default device switching, and audio profile management.

## System Interface

### WirePlumber (wpctl)

#### Listing devices and streams

`wpctl status` — tree view of all audio objects. Asterisk (*) marks defaults. Each entry shows: ID, name, volume level.

`wpctl inspect <ID>` — detailed key-value properties for a node. Key properties:
- `media.class` — node type: "Audio/Sink", "Audio/Source", "Audio/Duplex", "Stream/Output/Audio", "Stream/Input/Audio"
- `node.name` — internal name
- `node.description` — human-readable name
- `node.nick` — short name
- `application.name` — app name for streams
- `application.icon-name` — app icon for streams
- `device.api` — "alsa", "bluez5", etc.

#### Volume control

- `wpctl get-volume <ID>` — output: `Volume: 0.35` and optionally `[MUTED]`. Value is float where 1.0 = 100%.
- `wpctl set-volume <ID> <vol>` — accepts: absolute (`0.5`), percentage (`75%`), relative (`+5%`, `-10%`)
- `wpctl set-mute <ID> <on|off|toggle>`
- `wpctl set-default <ID>` — set default sink or source

Special IDs:
- `@DEFAULT_AUDIO_SINK@` — current default output
- `@DEFAULT_AUDIO_SOURCE@` — current default input

### PulseAudio (pactl) — fallback / compatibility

#### Listing

- `pactl --format json list sinks` — JSON array of sinks with: index, name, description, state, volume (per-channel: absolute 0-65536, percentage, dB), mute, channels, sample spec, properties
- `pactl --format json list sources` — same format for inputs
- `pactl --format json list sink-inputs` — active playback streams with: index, sink, client, volume, mute, properties
- `pactl --format json list source-outputs` — active recording streams
- `pactl --format json list cards` — audio cards with available profiles

#### Volume control

- `pactl get-sink-volume <sink>` — output: `Volume: front-left: 49152 / 75% / -7.50 dB, front-right: 49152 / 75% / -7.50 dB`
- `pactl set-sink-volume <sink> <vol>` — accepts: integer (0-65536), percentage (`75%`), decibel (`-5dB`), relative (`+10%`)
- `pactl set-sink-mute <sink> <1|0|toggle>`
- `pactl set-default-sink <sink>` — set default output
- `pactl set-default-source <source>` — set default input
- `pactl move-sink-input <stream-index> <sink>` — route a stream to a different output

#### Audio profiles (cards)

- `pactl list cards` — shows card with available profiles
- `pactl set-card-profile <card> <profile>` — switch profile

Bluetooth profiles on a card named `bluez_card.XX_XX_XX_XX_XX_XX`:
- `a2dp-sink` — stereo output, no mic (music)
- `headset-head-unit` — mono in+out with mic (calls)
- `off` — disable

ALSA surround profiles:
- `output:analog-stereo` — 2-channel
- `output:analog-surround-51` — 5.1 channel
- `output:analog-surround-71` — 7.1 channel

### PulseAudio D-Bus API (works with PipeWire via module-protocol-pulse)

**Discovery:** PulseAudio uses a private D-Bus bus, not the session bus.
1. Check `PULSE_DBUS_SERVER` env var — if set, use as D-Bus address directly
2. Otherwise: query `org.PulseAudio1` on session bus → get `/org/pulseaudio/server_lookup1` → read `Address` property from `org.PulseAudio.ServerLookup1` interface → returns address like `unix:path=/run/user/1000/pulse/dbus-socket`
3. Connect to that private bus address

#### org.PulseAudio.Core1 (object: `/org/pulseaudio/core1`)

Properties:
- `Sinks: Vec<ObjectPath>` — all sink object paths
- `Sources: Vec<ObjectPath>` — all source object paths
- `PlaybackStreams: Vec<ObjectPath>` — active playback streams
- `RecordStreams: Vec<ObjectPath>` — active recording streams
- `Cards: Vec<ObjectPath>` — audio cards
- `DefaultSink: ObjectPath` (R/W) — current default output
- `DefaultSource: ObjectPath` (R/W) — current default input
- `FallbackSink: ObjectPath` (R/W)
- `FallbackSource: ObjectPath` (R/W)

Signals:
- `NewSink(ObjectPath)` / `SinkRemoved(ObjectPath)`
- `NewSource(ObjectPath)` / `SourceRemoved(ObjectPath)`
- `NewPlaybackStream(ObjectPath)` / `PlaybackStreamRemoved(ObjectPath)`
- `NewRecordStream(ObjectPath)` / `RecordStreamRemoved(ObjectPath)`
- `NewCard(ObjectPath)` / `CardRemoved(ObjectPath)`
- `DefaultSinkUpdated(ObjectPath)`
- `DefaultSourceUpdated(ObjectPath)`

#### org.PulseAudio.Core1.Sink (object: `/org/pulseaudio/core1/sink{N}`)

Properties:
- `Index: u32`
- `Name: String`
- `Description: String`
- `Volume: Vec<u32>` (R/W) — per-channel volume, scale 0–65536 (65536 = 100%)
- `BaseVolume: u32` — reference "no amplification" level
- `Mute: bool` (R/W)
- `Channels: Vec<String>` — channel names (e.g. ["front-left", "front-right"])
- `ActivePort: ObjectPath`
- `Ports: Vec<ObjectPath>`
- `Card: ObjectPath`
- `HasFlatVolume: bool`
- `HasHardwareVolume: bool`
- `HasHardwareMute: bool`
- `VolumeSteps: u32`

Signals:
- `VolumeUpdated(Vec<u32>)` — fires on any volume change
- `MuteUpdated(bool)` — fires on mute toggle

#### org.PulseAudio.Core1.Source (object: `/org/pulseaudio/core1/source{N}`)

Same structure as Sink, for input devices.

#### org.PulseAudio.Core1.Stream (playback: `/org/pulseaudio/core1/playback_stream{N}`, record: `/org/pulseaudio/core1/record_stream{N}`)

Properties:
- `Index: u32`
- `Device: ObjectPath` — sink/source this stream is connected to
- `Client: ObjectPath`
- `Volume: Vec<u32>` (R/W) — per-channel volume
- `Mute: bool` (R/W)
- `Name: String`
- `BufferLatency: u64` — microseconds
- `DeviceLatency: u64` — microseconds

Signals:
- `VolumeUpdated(Vec<u32>)`
- `MuteUpdated(bool)`
- `DeviceUpdated(ObjectPath)` — stream moved to different device

#### org.PulseAudio.Core1.Card (object: `/org/pulseaudio/core1/card{N}`)

Properties:
- `Index: u32`
- `Name: String`
- `ActiveProfile: ObjectPath` (R/W)
- `Profiles: Vec<ObjectPath>`

#### org.PulseAudio.Core1.CardProfile (object: `/org/pulseaudio/core1/card{N}/profile{M}`)

Properties:
- `Index: u32`
- `Name: String` — e.g. "a2dp-sink", "headset-head-unit"
- `Description: String`
- `Priority: u32`
- `Available: bool`

**Note:** This D-Bus API works with PipeWire via `module-protocol-pulse`. PipeWire itself has no native D-Bus interface — it uses a custom binary protocol on `unix:/run/user/$UID/pipewire-0` which is not practical to implement directly.

### PipeWire native (pw-dump)

`pw-dump` returns full JSON graph. Each object:
```json
{
  "id": 42,
  "type": "PipeWire:Interface:Node",
  "info": {
    "props": {
      "node.name": "alsa_output.pci-0000_00_1b.0.analog-stereo",
      "node.description": "Built-in Audio Analog Stereo",
      "media.class": "Audio/Sink",
      "audio.channels": 2,
      "audio.rate": 48000
    }
  }
}
```

## Topics

- `audio.default_output` — current default sink (id, name, volume, mute)
- `audio.default_input` ��� current default source
- `audio.outputs` — list of all sinks
- `audio.inputs` �� list of all sources
- `audio.streams` — list of active playback/recording streams
- `audio.output.{id}.volume` — volume/mute for a specific output
- `audio.input.{id}.volume` — volume/mute for a specific input
- `audio.stream.{id}.volume` — volume/mute for a specific stream
- `audio.cards` — audio cards with available profiles

## Methods

- `audio.set_volume(node_id: u32, volume: f64)` — set volume (0.0 = silent, 1.0 = 100%, >1.0 = overamplified)
- `audio.set_mute(node_id: u32, muted: bool)` — set mute state
- `audio.set_default_output(node_id: u32)` — set default sink
- `audio.set_default_input(node_id: u32)` — set default source
- `audio.move_stream(stream_id: u32, target_node_id: u32)` — route a stream to a different output/input
- `audio.set_card_profile(card_id: u32, profile: String)` — switch audio card profile (e.g. A2DP vs HSP)

## Types

```rust
/// Type of audio node
enum AudioNodeType {
    /// Output device (speaker, headphone, etc.)
    Sink,
    /// Input device (microphone, line-in, etc.)
    Source,
    /// Bidirectional device
    Duplex,
}

/// Type of audio stream
enum StreamDirection {
    /// Playback stream (app -> output)
    Playback,
    /// Recording stream (input -> app)
    Recording,
}

/// Audio device API backend
enum AudioApi {
    Alsa,
    Bluez,
    Other(String),
}

/// An audio output or input device
struct AudioDevice {
    /// PipeWire/PulseAudio node ID
    id: u32,
    /// Internal name (e.g. "alsa_output.pci-0000_00_1b.0.analog-stereo")
    name: String,
    /// Human-readable description (e.g. "Built-in Audio Analog Stereo")
    description: String,
    node_type: AudioNodeType,
    api: AudioApi,
    /// Volume level (0.0 = silent, 1.0 = 100%)
    volume: f64,
    muted: bool,
    /// Number of audio channels
    channels: u32,
    /// Sample rate in Hz
    sample_rate: u32,
    /// Whether this is the current default device
    is_default: bool,
}

/// An active audio stream (application)
struct AudioStream {
    /// Stream ID
    id: u32,
    /// Application name
    app_name: String,
    /// Application icon name (freedesktop icon)
    app_icon: String,
    direction: StreamDirection,
    /// Target device ID this stream is connected to
    target_device_id: u32,
    volume: f64,
    muted: bool,
    channels: u32,
}

/// Audio card with switchable profiles
struct AudioCard {
    id: u32,
    name: String,
    description: String,
    /// Currently active profile name
    active_profile: String,
    /// All available profiles
    profiles: Vec<AudioProfile>,
}

/// A profile available on an audio card
struct AudioProfile {
    /// Profile name (e.g. "a2dp-sink", "headset-head-unit", "output:analog-surround-51")
    name: String,
    /// Human-readable description
    description: String,
    /// Whether this profile is currently available
    available: bool,
}

/// Volume state for a device or stream
struct VolumeState {
    /// 0.0 = silent, 1.0 = 100%
    volume: f64,
    muted: bool,
}
```

## Icons

Volume state:
- `audio-volume-muted-symbolic` — muted
- `audio-volume-low-symbolic` — low (~0-33%)
- `audio-volume-medium-symbolic` — medium (~33-66%)
- `audio-volume-high-symbolic` — high (~66-100%)
- `audio-volume-overamplified-symbolic` — over 100%

Device type:
- `audio-speakers-symbolic` — speakers
- `audio-headphones-symbolic` — wired headphones
- `audio-headset-symbolic` — headset with microphone
- `audio-input-microphone-symbolic` — microphone

Selection logic:
```
if muted -> audio-volume-muted-symbolic
elif volume < 0.33 -> audio-volume-low-symbolic
elif volume < 0.66 -> audio-volume-medium-symbolic
elif volume <= 1.0 -> audio-volume-high-symbolic
else -> audio-volume-overamplified-symbolic
```

All icons above are available in Adwaita icon theme.

## Change Detection

**PulseAudio D-Bus signals (preferred):** Typed signals on each object — no parsing needed:
- `Core1`: `NewSink`, `SinkRemoved`, `NewSource`, `SourceRemoved`, `NewPlaybackStream`, `PlaybackStreamRemoved`, `DefaultSinkUpdated`, `DefaultSourceUpdated`
- `Sink/Source`: `VolumeUpdated(Vec<u32>)`, `MuteUpdated(bool)`
- `Stream`: `VolumeUpdated`, `MuteUpdated`, `DeviceUpdated`

Works with PipeWire via `module-protocol-pulse`.

**pactl subscribe (alternative):** Real-time text event stream. Output format: `Event 'change' on sink #0`

Event types:
- `new` / `remove` — device or stream added/removed (hotplug, app start/stop)
- `change` — property changed (volume, mute, default device, profile switch)
- Object types: sink, source, sink-input, source-output, card, client, module

Usage: `pactl subscribe` outputs one line per event, indefinitely. Works with both PipeWire and native PulseAudio.

**pw-mon:** PipeWire native monitor. Outputs: `added`, `changed`, `removed` events with object type and ID. More granular but harder to parse.

**Recommended approach:** Use PulseAudio D-Bus signals for typed change detection. Fall back to `pactl subscribe` if D-Bus connection fails.

## Features

- List all audio outputs (sinks) with volume, mute, channel count, sample rate
- List all audio inputs (sources) with same properties
- List active playback/recording streams with app name, icon, volume
- Per-device volume control (absolute and relative)
- Per-stream volume control
- Mute/unmute per device and per stream
- Default output/input device switching
- Stream routing (move stream to different device)
- Audio card profile switching (A2DP vs HSP for Bluetooth)
- ALSA surround sound profiles (stereo, 5.1, 7.1)
- Device hotplug detection
- Stream start/stop detection
- Over-amplification support (volume > 100%)
- Per-channel volume (future: independent L/R control)
- Peak level metering (future)

## Notes

- WirePlumber/wpctl is the primary interface on modern systems; pactl works via PipeWire's PulseAudio compatibility layer
- Volume is float: 0.0 = silent, 1.0 = 100%, values > 1.0 are overamplified
- PulseAudio uses 0-65536 integer scale; conversion: `pa_volume / 65536.0`
- Bluetooth profile switching requires the card ID, not the sink ID
- `pactl --format json` requires `LC_NUMERIC=C` for reliable float parsing in some locales
- pw-dump gives the full PipeWire graph but is heavyweight; prefer wpctl/pactl for targeted queries
