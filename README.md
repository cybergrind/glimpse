# Glimpse

Glimpse is a Wayland desktop shell toolkit for Niri. It provides the
desktop pieces that sit around the compositor: a panel, wallpaper and
backdrop surfaces, a lock screen, night light, and idle behavior.

The goal is a desktop that feels cohesive without becoming a full desktop
environment. Glimpse keeps configuration in readable files, ships polished
defaults, and lets you replace the parts that should reflect your setup.

## Contents

- [Why Glimpse Exists](#why-glimpse-exists)
- [What's Inside](#whats-inside)
- [Installation](#installation)
- [Configuration](#configuration)
- [Wallpaper And Backdrop](#wallpaper-and-backdrop)
- [Lock Screen](#lock-screen)
- [Night Light](#night-light)
- [Idle Policy](#idle-policy)
- [Theming](#theming)
- [Documentation](#documentation)

## Why Glimpse Exists

Glimpse exists because a beautiful tiling desktop should not feel unfinished.

The project grew from a long KDE and GNOME desktop background. Niri brought
the workflow that felt right, but it still needed the surrounding desktop
layer: status, controls, wallpaper, locking, idle policy, and night color.
Glimpse is that layer for a Niri-first Wayland session.

Glimpse optimizes for:

| Value | What it means |
|---|---|
| **Niri-first workflow** | Built for a modern Wayland session around Niri. |
| **Professional feel** | Polished, restrained defaults for daily use. |
| **Small pieces** | Run the shell, wallpaper, lock, sunset, and idle pieces independently. |
| **Readable config** | Keep the desktop in TOML and CSS files that are practical to version. |
| **Daily comfort** | Make lock, idle, night light, wallpaper, and panel status work together. |

## What's Inside

| Component | Purpose |
|---|---|
| `glimpse-shell` | GTK4 layer-shell panel with built-in and custom applets. |
| `glimpse-wallpaper` | Wallpaper and blurred backdrop daemon. |
| `glimpse-lock` | Session lock screen with PAM authentication and CSS theming. |
| `glimpse-sunset` | Night-light daemon with fixed or automatic schedules. |
| `glimpse-idle` | Idle policy daemon for lock, display power, suspend, and commands. |

All runtime pieces read the same Glimpse configuration model. A normal setup
keeps the file at:

```text
~/.config/glimpse/config.toml
```

## Installation

Glimpse is packaged for Arch-based systems as a prebuilt AUR package:

```sh
yay -S glimpse-desktop-bin
```

Use your preferred AUR helper if you do not use `yay`.

The package installs:

```text
glimpse-shell
glimpse-wallpaper
glimpse-lock
glimpse-sunset
glimpse-idle
```

It also installs systemd user services and the default PAM service file for
`glimpse-lock`.

### Enable Services

For a normal Niri desktop, enable the shell, lock screen, night light, and idle
policy:

```sh
systemctl --user enable --now glimpse-shell.service
systemctl --user enable --now glimpse-lock.service
systemctl --user enable --now glimpse-sunset.service
systemctl --user enable --now glimpse-idle.service
```

`glimpse-shell.service` wants `glimpse-wallpaper.service`, so starting the shell
also starts the wallpaper daemon. Enable `glimpse-wallpaper.service` directly
only if you want the wallpaper daemon without the shell.

Check service state:

```sh
systemctl --user status glimpse-shell.service
systemctl --user status glimpse-lock.service
systemctl --user status glimpse-sunset.service
systemctl --user status glimpse-idle.service
```

View logs:

```sh
journalctl --user -u glimpse-shell.service -e
```

Replace `glimpse-shell.service` with the service you are checking.

### Version Check

Each command supports `--version`:

```sh
glimpse-shell --version
glimpse-wallpaper --version
glimpse-lock --version
glimpse-sunset --version
glimpse-idle --version
```

## Configuration

Glimpse starts with defaults when no config file is present. The default shell
has one top panel, built-in theme defaults, and the standard applet layout.

Config discovery order:

| Priority | Path |
|---|---|
| **1** | `GLIMPSE_CONFIG` environment variable |
| **2** | `./config.toml` in the current directory |
| **3** | `$XDG_CONFIG_HOME/glimpse/config.toml` |
| **4** | `$HOME/.config/glimpse/config.toml` when `XDG_CONFIG_HOME` is unset |

Create:

```text
~/.config/glimpse/config.toml
```

A compact starter config:

```toml
theme = "adwaita"
theme_mode = "auto"

[[panels]]
position = "top"
size = 36
left = ["pager", "mpris"]
center = ["clock"]
right = ["network", "battery", "session"]

[location]
provider = "static"
latitude = 52.2297
longitude = 21.0122
```

### Panel Layout

Default panel layout:

- **Left:** `pager`, `mpris`
- **Center:** `clock`, `weather`, `notifications`
- **Right:** `tray`, `removable`, `clipboard`, `keyboard`, `privacy`,
  `bluetooth`, `network`, `brightness`, `audio`, `battery`, `session`

Panel options:

| Key | Purpose | Values |
|---|---|---|
| `position` | Screen edge for the panel. | `top`, `bottom`, `left`, `right` |
| `size` | Panel thickness in pixels. | Integer |
| `monitor` | Optional output name. | Example: `eDP-1` |
| `theme_mode` | Per-panel color mode. | `auto`, `light`, `dark` |
| `left`, `center`, `right` | Applet names for each section. | Array of names |

Use `"..."` inside a panel section to keep the default applets for that
section:

```toml
[[panels]]
position = "top"
left = ["...", "screenshot"]
center = ["clock"]
right = ["network", "battery", "..."]
```

### Built-In Applets

| Applet | Purpose |
|---|---|
| `audio` | Volume, mute state, output device, and microphone indicator. |
| `battery` | Battery percentage, charging state, and power profile. |
| `bluetooth` | Bluetooth state and connected devices. |
| `brightness` | Screen brightness with scroll control. |
| `clipboard` | Clipboard history. |
| `clock` | Time, date, calendar, and optional world clocks. |
| `command` | A button or menu that runs commands. |
| `exec` | A live custom status widget from your script or program. |
| `keyboard` | Current keyboard layout. |
| `mpris` | Media player status and controls. |
| `network` | Wi-Fi, wired network, and VPN status. |
| `notifications` | Notification center and popups. |
| `pager` | Workspaces and windows. |
| `privacy` | Camera, microphone, screen sharing, and location indicators. |
| `removable` | USB drives and removable storage. |
| `session` | Lock, logout, suspend, restart, and shutdown. |
| `tray` | Status notifier icons. |
| `weather` | Current weather and forecast. |

Configure an applet with `[applets.<name>]`:

```toml
[applets.clock]
format = "%H:%M"
tooltip = "%A, %-d %B %Y"

[[applets.clock.timezones]]
label = "Tokyo"
timezone = "Asia/Tokyo"
```

Create named applets with `extends`:

```toml
[[panels]]
position = "top"
right = ["terminal", "screenshot", "network", "battery"]

[applets.terminal]
extends = "command"
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]

[applets.screenshot]
extends = "command"
icon = "camera-photo-symbolic"
tooltip = "Copy area screenshot"
command = ["/bin/sh", "-c", "grim -g \"$(slurp)\" - | wl-copy"]
```

`command` applets run a command on click and can expose a right-click menu.
`exec` applets run an external process that speaks the Glimpse applet protocol.

## Wallpaper And Backdrop

`glimpse-wallpaper` uses `[wallpaper]` and `[backdrop]` from the shared config.

Solid color:

```toml
[wallpaper]
color = "#101010"
fit = "cover"
transition_ms = 800

[backdrop]
enabled = true
blur_radius = 24
```

Image wallpaper:

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
| `cover` | Fill the output while preserving aspect ratio. |
| `contain` | Fit the full image while preserving aspect ratio. |
| `fill` | Stretch the image to the output. |

When `[backdrop]` is enabled and `backdrop.path` is omitted, Glimpse derives
the backdrop from `wallpaper.path`.

## Lock Screen

`glimpse-lock` listens for logind lock requests. Keep the service running and
trigger locks with:

```sh
loginctl lock-session
```

Add lock settings under `[lock]` in the shared config:

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

If you do not set a lock background, Glimpse uses the wallpaper config as the
fallback.

Preview lock styling without taking a real session lock:

```sh
glimpse-lock --preview
```

Export starter lock CSS:

```sh
glimpse-lock --export-css
```

## Night Light

`glimpse-sunset` applies a warmer display temperature on a schedule. It uses
`[night_light]` and, for automatic scheduling, `[location]`.

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
| `off` | Disable night light. |
| `automatic` | Use location-based sunset and sunrise. |
| `schedule` | Use `start_time` and `end_time`. |

## Idle Policy

`glimpse-idle` runs commands after the session has been idle for configured
timeouts. It supports separate AC and battery profiles.

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
| `timeout` | Idle timeout in seconds. |
| `on_idle` | Shell command run when the timeout is reached. |
| `on_resume` | Shell command run when input resumes. |
| `respect_inhibitors` | Optional per-listener override for idle inhibitors. |

## Theming

Glimpse loads the built-in base CSS first, then loads your selected theme on
top of it.

User themes live in:

```text
~/.config/glimpse/themes/
```

Select a shell theme by file name without `.css`:

```toml
theme = "my-theme"
theme_mode = "auto"
```

This loads:

```text
~/.config/glimpse/themes/my-theme.css
```

Override the theme file directly with `GLIMPSE_THEME`:

```sh
GLIMPSE_THEME=/home/alex/.config/glimpse/themes/test.css glimpse-shell
```

Theme modes:

| Value | Behavior |
|---|---|
| `auto` | Follow the current interface color scheme. |
| `light` | Force light styling. |
| `dark` | Force dark styling. |

Starter shell CSS:

```css
:root {
  --accent-bg: #3584e4;
  --popover-padding: 14px;
}

.panel {
  background: rgba(20, 20, 20, 0.82);
  color: #f4f4f4;
}

.applet {
  padding: 0 8px;
}

.applet:hover {
  background: rgba(255, 255, 255, 0.08);
}
```

Lock screen CSS is configured separately:

```toml
[lock]
css_path = "themes/lock.css"
```

Relative lock CSS paths resolve from the Glimpse config directory. The default
path is:

```text
~/.config/glimpse/themes/lock.css
```

## Documentation

The documentation site is built with VitePress and deployed to GitHub Pages:

```text
https://alex-oleshkevich.github.io/glimpse/
```

Run the docs locally:

```sh
cd docs
npm ci
npm run docs:dev
```

Build the production docs:

```sh
cd docs
npm run docs:build
```

The GitHub Pages workflow is `.github/workflows/docs.yml`.

## More Docs

| Topic | Link |
|---|---|
| Motivation | [docs/motivation.md](docs/motivation.md) |
| Installation | [docs/installation.md](docs/installation.md) |
| Configuration | [docs/configuration.md](docs/configuration.md) |
| Applets | [docs/applets/index.md](docs/applets/index.md) |
| Custom applets | [docs/custom-applets/index.md](docs/custom-applets/index.md) |
| Theming | [docs/theming.md](docs/theming.md) |
| Lock screen | [docs/lock.md](docs/lock.md) |
| Wallpaper | [docs/wallpaper.md](docs/wallpaper.md) |
| Idle policy | [docs/idle.md](docs/idle.md) |
| Night light | [docs/sunset.md](docs/sunset.md) |
