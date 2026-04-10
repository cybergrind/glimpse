# glimpse-wallpaper

A Wayland layer-shell wallpaper daemon for the Glimpse desktop environment. Supports static images, image directories, solid colors, looping video, time-of-day schedules, Apple Dynamic Desktop (HEIC), crossfade transitions, and independent configuration per monitor.

## Configuration

All settings live under the `[wallpaper]` table in `~/.config/glimpse/config.toml`.

```toml
[wallpaper]
mode = "image"
path = "/path/to/wallpaper.jpg"
content_fit = "cover"
transition_ms = 800
```

### Options

| Key | Type | Default | Description |
|---|---|---|---|
| `mode` | string | `"image"` | Wallpaper mode. See [Modes](#modes). |
| `path` | string | — | File or directory path. Required for `image`, `directory`, `video`, `heic`. |
| `color` | string | `"#000000"` | Fallback solid color in `#rgb` or `#rrggbb` format. |
| `content_fit` | string | `"cover"` | How the image fills the screen: `fill`, `contain`, or `cover`. |
| `interval_seconds` | integer | `300` | Seconds between image changes in `directory` mode. |
| `order` | string | `"sorted"` | Image order in `directory` mode: `sorted` or `random`. |
| `recursive` | bool | `false` | Scan subdirectories in `directory` mode. |
| `looped` | bool | `true` | Loop the video in `video` mode. |
| `muted` | bool | `true` | Mute audio in `video` mode. |
| `transition_ms` | integer | `800` | Crossfade duration in milliseconds between images. Set to `0` for instant cuts. |

---

## Modes

### `image`

Displays a single static image.

```toml
[wallpaper]
mode = "image"
path = "/home/alex/wallpapers/mountain.jpg"
content_fit = "cover"
```

### `directory`

Cycles through all images in a directory at a fixed interval. Supports subdirectory scanning and random ordering.

```toml
[wallpaper]
mode = "directory"
path = "/home/alex/wallpapers"
interval_seconds = 300
order = "random"
recursive = true
content_fit = "cover"
transition_ms = 1000
```

**Supported formats:** JPEG, PNG, BMP, GIF, TIFF, WebP, HEIC, HEIF.

### `color`

Fills the screen with a solid color. Useful as a minimal background or fallback.

```toml
[wallpaper]
mode = "color"
color = "#1e1e2e"
```

### `video`

Plays a looping video file as the wallpaper background using GStreamer.

```toml
[wallpaper]
mode = "video"
path = "/home/alex/wallpapers/nature-loop.mp4"
content_fit = "cover"
looped = true
muted = true
```

**Requirements:** The `gst-plugin-gtk4` GStreamer plugin (`gst-plugins-rs`) must be installed for the `gtk4paintablesink` element.

Video looping uses GStreamer's gapless playback mechanism (`about-to-finish` signal) for seamless transitions without frame flashes.

### `schedule`

Displays different images at different times of day. Frames are defined as an ordered list of `[[wallpaper.frames]]` entries; the active frame is the last one whose `time` is ≤ the current time. If the current time is before all frames (e.g. 02:00 with frames starting at 06:00), the last frame carries over from the previous day.

```toml
[wallpaper]
mode = "schedule"
content_fit = "cover"
transition_ms = 2000

[[wallpaper.frames]]
time = "06:00"
path = "/wallpapers/morning.jpg"

[[wallpaper.frames]]
time = "12:00"
path = "/wallpapers/noon.jpg"

[[wallpaper.frames]]
time = "18:00"
path = "/wallpapers/evening.jpg"

[[wallpaper.frames]]
time = "21:00"
path = "/wallpapers/night.jpg"
```

The schedule is polled every 60 seconds. Frame changes crossfade if `transition_ms > 0`.

### `workspace`

Changes the wallpaper based on the active workspace on the monitor. Only valid inside a `[[wallpaper.monitors]]` block. Requires running under [niri](https://github.com/YaLTeR/niri) (uses `$NIRI_SOCKET`).

Each `[[wallpaper.monitors.workspaces]]` entry maps a 1-based workspace index to a wallpaper config. Any mode except `workspace` itself is valid inside a slot.

```toml
[[wallpaper.monitors]]
name = "DP-1"
mode = "workspace"

[[wallpaper.monitors.workspaces]]
index = 1
mode = "image"
path = "/wallpapers/coding.jpg"
content_fit = "cover"

[[wallpaper.monitors.workspaces]]
index = 2
mode = "color"
color = "#0d1117"

[[wallpaper.monitors.workspaces]]
index = 3
mode = "video"
path = "/wallpapers/nature.mp4"
```

Workspace switches crossfade with a 600 ms transition. If `$NIRI_SOCKET` is not set (non-niri compositor), the first workspace slot is shown permanently with no switching.

### `heic`

Plays Apple Dynamic Desktop wallpapers — multi-frame HEIC files with an embedded time-of-day schedule. Frames are extracted to `~/.cache/glimpse/wallpapers/<hash>/` on first use and reused on subsequent runs.

```toml
[wallpaper]
mode = "heic"
path = "/home/alex/wallpapers/big-sur-coastline.heic"
content_fit = "cover"
transition_ms = 2000
```

**How it works:** The HEIC file contains XMP metadata with an `apple_desktop:h24` schedule — a list of frame indices mapped to time-of-day fractions (0.0 = midnight, 1.0 = next midnight). The frame with the smallest circular distance to the current time fraction is selected. The schedule is re-evaluated every 60 seconds.

Solar-based schedules (`apple_desktop:solar`) fall back to the light-mode frame index. Full solar support (requiring location from GeoClue2) is not yet implemented.

---

## Per-Monitor Configuration

Each connected monitor can override any root setting. Monitors are matched by connector name (e.g. `DP-1`, `HDMI-1`).

```toml
[wallpaper]
mode = "image"
path = "/wallpapers/default.jpg"
content_fit = "cover"

# Primary monitor: looping video
[[wallpaper.monitors]]
name = "DP-1"
mode = "video"
path = "/wallpapers/nature-loop.mp4"

# Secondary monitor: solid color
[[wallpaper.monitors]]
name = "HDMI-1"
mode = "color"
color = "#0d1117"
```

Any field not specified in a monitor block inherits from the root `[wallpaper]` config. Monitor windows are automatically opened and closed as monitors are connected and disconnected (hot-plug).

To find connector names on a running Niri session:

```bash
niri msg outputs
```

---

## Crossfade Transitions

Image and schedule modes support smooth crossfade transitions between wallpapers. The transition uses a GTK stack with two alternating pages — the incoming image is rendered off-screen before the fade begins.

```toml
[wallpaper]
transition_ms = 1000   # 1 second crossfade
```

Set `transition_ms = 0` to disable crossfading entirely.

---

## Content Fit

Controls how images are scaled to fill the monitor:

| Value | Behaviour |
|---|---|
| `fill` | Stretches the image to exactly fill the screen, ignoring aspect ratio. |
| `contain` | Scales the image to fit entirely within the screen, preserving aspect ratio. Letterboxes if needed. |
| `cover` | Scales the image to cover the entire screen, preserving aspect ratio. Crops if needed. |

---

## Logging

Log level is controlled by the `GLIMPSE_WALLPAPER_LOG_LEVEL` environment variable:

```bash
GLIMPSE_WALLPAPER_LOG_LEVEL=debug glimpse-wallpaper
```

Accepted values: `error`, `warn`, `info` (default), `debug`, `trace`.

---

## HEIC Frame Cache

Extracted HEIC frames are stored at:

```
~/.cache/glimpse/wallpapers/<seahash>/frame-0000.png
~/.cache/glimpse/wallpapers/<seahash>/frame-0001.png
...
```

The cache key is a seahash of the raw HEIC file bytes — changing the file invalidates the cache automatically. To manually clear all cached frames:

```bash
rm -rf ~/.cache/glimpse/wallpapers/
```
