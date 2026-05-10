# Configuration

The main config file is `~/.config/glimpse/config.toml`. It controls the panel, applets, location, night light, and idle behavior.

## Start With A Panel

This is a small, useful panel:

```toml
[[panels]]
output = "eDP-1"
position = "top"
height = 36
left = ["pager", "..."]
center = ["clock"]
right = ["network", "battery", "weather", "session"]
```

The special name `"..."` means “keep the default applets for this section here.” It lets you add your own items without copying the whole default layout.

## Panel Options

| Option | What it does |
|---|---|
| `output` | Monitor name. Leave it out if one panel should appear on the default output. |
| `position` | `top` or `bottom`. |
| `height` | Panel height in pixels. |
| `left` | Applets on the left side. |
| `center` | Applets in the center. |
| `right` | Applets on the right side. |

## Built-In Applets

| Applet | Use it for |
|---|---|
| `audio` | Volume status and controls. |
| `battery` | Battery percentage and charging status. |
| `bluetooth` | Bluetooth status and devices. |
| `brightness` | Screen brightness. |
| `clipboard` | Clipboard history. |
| `clock` | Time and calendar. |
| `keyboard` | Current keyboard layout. |
| `mpris` | Media player status. |
| `network` | Wi-Fi and wired network status. |
| `notifications` | Notification center. |
| `pager` | Windows of the focused workspace (or workspace dots on Hyprland). |
| `workspaces-pager` | Per-monitor workspaces with names. |
| `privacy` | Camera, microphone, and screen sharing indicators. |
| `removable` | USB and removable drives. |
| `session` | Lock, logout, suspend, restart, and shutdown actions. |
| `tray` | Status notifier icons. |
| `weather` | Current weather. |
| `command` | A button or menu that runs commands. |
| `exec` | A live custom applet powered by your own script. |

## Configure An Applet

Applet settings live under `[applets.name]`.

```toml
[applets.weather]
location = "Warsaw"

[applets.terminal]
extends = "command"
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]
```

Use names that make sense to you. Then place those names in a panel section:

```toml
right = ["weather", "terminal", "network", "battery"]
```

## Location

Location is used by weather and automatic night light.

For automatic location:

```toml
[location]
provider = "geoclue"
```

For a fixed place:

```toml
[location]
provider = "static"
latitude = 52.2297
longitude = 21.0122
```

Static location is useful on desktops, privacy-focused setups, or systems without a location provider.

## Keep Config Tidy

Use one section per topic:

```toml
[[panels]]
position = "top"
left = ["pager"]
center = ["clock"]
right = ["network", "battery", "session"]

[location]
provider = "static"
latitude = 52.2297
longitude = 21.0122

[applets.clock]
format = "%H:%M"
```

Put wallpaper options in [Wallpaper](./wallpaper.md) and lock screen options in [Lock](./lock.md). That keeps your main file readable.
