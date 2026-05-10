# Glimpse

Glimpse is a Wayland desktop shell toolkit for building a personal status panel and the small session daemons that usually surround it. The repository provides:

| Component | Purpose |
|---|---|
| `glimpse-shell` | GTK4 layer-shell status panel with built-in and command-driven applets |
| `glimpse-sunset` | Night-light daemon with automatic sunset/sunrise scheduling |
| `glimpse-lock` | Session lock screen with configurable background, controls, and CSS |
| `glimpse-idle` | Idle policy daemon for locking, monitor power, suspend, and custom commands |
| `glimpse-wallpaper` | Wallpaper and blurred backdrop daemon |

Glimpse reads one shared TOML configuration file. The same file can configure the panel, applets, theme, night light, lock screen, idle behavior, and wallpaper.

## Installation

On Arch Linux, install the AUR package:

```sh
yay -S glimpse-desktop-bin
```

The package provides the panel and companion apps:

```sh
glimpse-shell
glimpse-sunset
glimpse-lock
glimpse-idle
glimpse-wallpaper
```

Systemd user service files are included for the background apps:

```sh
systemctl --user enable --now glimpse-sunset.service
systemctl --user enable --now glimpse-lock.service
systemctl --user enable --now glimpse-idle.service
systemctl --user enable --now glimpse-wallpaper.service
```

## Configuration

Glimpse Shell runs without a config file. When no config file is found, it starts with one top panel, the built-in theme defaults, and the default applet layout.

Glimpse looks for optional shared config in this order:

| Priority | Path |
|---|---|
| **1** | `GLIMPSE_CONFIG` environment variable |
| **2** | `./config.toml` in the current directory |
| **3** | `$XDG_CONFIG_HOME/glimpse/config.toml` |
| **4** | `$HOME/.config/glimpse/config.toml` when `XDG_CONFIG_HOME` is unset |

Default panel layout:

| Section | Applets |
|---|---|
| **Left** | `pager`, `mpris` |
| **Center** | `clock`, `weather`, `notifications` |
| **Right** | `tray`, `removable`, `clipboard`, `keyboard`, `privacy`, `bluetooth`, `network`, `brightness`, `audio`, `battery`, `session` |

Use `config.toml` when you want to override those defaults:

```toml
theme = "adwaita"
theme_mode = "auto"

[[panels]]
position = "top"
size = 36
left = ["pager", "mpris"]
center = ["clock", "weather", "notifications"]
right = ["tray", "removable", "clipboard", "keyboard", "privacy", "bluetooth", "network", "brightness", "audio", "battery", "session"]
```

Panel options:

| Key | Purpose | Values |
|---|---|---|
| `position` | Screen edge for the panel | `top`, `bottom`, `left`, `right` |
| `size` | Panel thickness in pixels | Integer |
| `monitor` | Optional output name | Example: `eDP-1` |
| `theme_mode` | Per-panel color mode | `auto`, `light`, `dark` |
| `left`, `center`, `right` | Applet names for each section | Array of applet names |

Use `"..."` inside a panel section to include the default applets for that section:

```toml
[[panels]]
position = "top"
left = ["...", "screenshot"]
center = ["clock"]
right = ["network", "battery", "..."]
```

### Applets And Applet Configs

Built-in applet names:

```text
audio
battery
bluetooth
brightness
clipboard
clock
command
exec
keyboard
mpris
network
notifications
pager
privacy
removable
session
tray
weather
workspaces-pager
```

Configure an applet with `[applets.<name>]`. When `<name>` is a built-in applet, the config applies to that built-in instance:

```toml
[applets.clock]
label_format = "%H:%M"
tooltip_format = "%A, %-d %B %Y"

[[applets.clock.timezones]]
name = "Warsaw"
timezone = "Europe/Warsaw"
```

Create a named applet by using `extends` and placing that name in a panel section:

```toml
[[panels]]
position = "top"
right = ["screenshot", "network", "battery"]

[applets.screenshot]
extends = "command"
icon = "camera-photo-symbolic"
tooltip = "Copy area screenshot"
command = ["/bin/sh", "-c", "grim -g \"$(slurp)\" - | wl-copy"]

[[applets.screenshot.menu]]
label = "Copy screen"
command = ["/bin/sh", "-c", "grim - | wl-copy"]
```

`command` applets run a command on click and can expose a right-click menu. `exec` applets run an external applet process that speaks the Glimpse applet protocol.

See also:

- [Panel and applet configuration](docs/configuration.md)
- [Custom applets](docs/custom-applets/index.md)
- [Command applets](docs/custom-applets/command.md)
- [Exec applets](docs/custom-applets/exec.md)

## Theming

Glimpse loads the built-in base CSS first, then loads the selected user theme on top of it. Put user themes in:

```text
$XDG_CONFIG_HOME/glimpse/themes/
```

When `XDG_CONFIG_HOME` is unset, use:

```text
$HOME/.config/glimpse/themes/
```

Select a theme by file name without the `.css` extension:

```toml
theme = "my-theme"
theme_mode = "auto"
```

This loads:

```text
$XDG_CONFIG_HOME/glimpse/themes/my-theme.css
```

Override the theme file directly with `GLIMPSE_THEME`:

```sh
GLIMPSE_THEME=/home/alex/.config/glimpse/themes/test.css glimpse-shell
```

Theme modes:

| Value | Behavior |
|---|---|
| `auto` | Follow the current interface color scheme |
| `light` | Force light styling |
| `dark` | Force dark styling |

User theme CSS should override variables and classes from the base theme:

```css
:root {
  --accent-bg: #3584e4;
  --popover-padding: 14px;
}

.clock-popover {
  border-radius: 8px;
}
```

## Sunset App

`glimpse-sunset` applies a warmer display temperature on a schedule. It uses the shared `[night_light]` config and, for automatic scheduling, `[location]`.

Automatic sunset setup:

```toml
[location]
provider = "static"
latitude = 52.2297
longitude = 21.0122

[night_light]
schedule = "automatic"
temperature = 4200
transition_minutes = 15
```

Fixed schedule setup:

```toml
[night_light]
schedule = "schedule"
start_time = "20:30"
end_time = "07:00"
temperature = 4200
transition_minutes = 15
```

Night-light schedule values:

| Value | Behavior |
|---|---|
| `off` | Disable night light |
| `automatic` | Use location-based sunset and sunrise |
| `schedule` | Use `start_time` and `end_time` |

See [Sunset](docs/sunset.md) for more examples and troubleshooting.

## Lock App

`glimpse-lock` provides the session lock screen. It uses the shared `[lock]` config and can be triggered from the session applet or with `loginctl lock-session`.

Enable the service:

```sh
systemctl --user enable --now glimpse-lock.service
```

Example config:

```toml
[lock]
pam_service = "glimpse-lock"
css_path = "themes/lock.css"

[lock.background]
path = "/home/alex/Pictures/wallpapers/night-city.jpg"
fit = "cover"
blur_radius = 24
dim = 0.35

[lock.clock]
enabled = true
time_format = "%H:%M"
date_format = "%A, %B %-d"

[lock.controls]
buttons = ["wifi", "input", "weather", "battery", "power"]
```

Lock control buttons:

```text
wifi
input
weather
battery
power
```

### Lock Theming

The lock screen has its own CSS file, configured by `lock.css_path`. Relative paths are resolved from the Glimpse config directory.

```css
.lock-card {
  border-radius: 12px;
}

.lock-clock-time {
  font-size: 64px;
  font-weight: 700;
}
```

See [Lock](docs/lock.md) for export commands, preview workflow, and CSS notes.

## Idle App

`glimpse-idle` runs commands after the session has been idle for configured timeouts. It supports separate AC and battery profiles.

Enable the service:

```sh
systemctl --user enable --now glimpse-idle.service
```

Example laptop config:

```toml
[idle]
enabled = true
respect_inhibitors = true

[idle.profiles.ac]
listeners = [
  { timeout = 300, on_idle = "loginctl lock-session" },
  { timeout = 600, on_idle = "niri msg action power-off-monitors", on_resume = "niri msg action power-on-monitors" },
  { timeout = 1800, on_idle = "systemctl suspend" },
]

[idle.profiles.battery]
listeners = [
  { timeout = 300, on_idle = "loginctl lock-session" },
  { timeout = 600, on_idle = "niri msg action power-off-monitors", on_resume = "niri msg action power-on-monitors" },
  { timeout = 900, on_idle = "systemctl suspend" },
]
```

Listener options:

| Key | Purpose |
|---|---|
| `timeout` | Idle timeout in seconds |
| `on_idle` | Shell command run when the timeout is reached |
| `on_resume` | Shell command run when input resumes |
| `respect_inhibitors` | Optional per-listener override for idle inhibitors |

See [Idle](docs/idle.md) for command examples and inhibitor behavior.

## Wallpaper App

`glimpse-wallpaper` sets the desktop wallpaper and optional blurred backdrop. It uses `[wallpaper]` and `[backdrop]`.

Enable the service:

```sh
systemctl --user enable --now glimpse-wallpaper.service
```

Solid color config:

```toml
[wallpaper]
color = "#101010"
fit = "cover"
transition_ms = 800

[backdrop]
enabled = true
blur_radius = 24
```

Image config:

```toml
[wallpaper]
color = "#101010"
path = "/home/alex/Pictures/wallpapers/coast.jpg"
fit = "cover"
transition_ms = 800

[backdrop]
enabled = true
blur_radius = 24
```

Fit modes:

| Value | Behavior |
|---|---|
| `cover` | Fill the output while preserving aspect ratio |
| `contain` | Fit the full image while preserving aspect ratio |
| `fill` | Stretch the image to the output |

Backdrop config:

```toml
[backdrop]
enabled = true
path = "/home/alex/Pictures/wallpapers/backdrop.jpg"
blur_radius = 24
```

When `backdrop.path` is omitted, Glimpse derives the backdrop from the wallpaper image.

See [Wallpaper](docs/wallpaper.md) for reload behavior and wallpaper tips.
