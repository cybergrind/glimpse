# Applets

Applets are the small things in your panel: clock, battery, network, media, weather, custom launchers, and status widgets.

Place applets in a panel section:

```toml
[[panels]]
left = ["pager", "mpris"]
center = ["clock"]
right = ["network", "battery", "session"]
```

Configure an applet under `[applets.name]`:

```toml
[applets.clock]
format = "%H:%M"
tooltip = "%A, %B %-d"
```

To create a second copy or a custom-named applet, use `extends`:

```toml
[applets.short-battery]
extends = "battery"
label_on_battery = "{percentage}%"

[[panels]]
right = ["short-battery", "session"]
```

## Common Format Fields

Many applets support `label`, `label_format`, `tooltip`, or `tooltip_format`. Short aliases like `label` and `tooltip` are accepted by most applets.

An empty label means the applet shows only its icon.

## Built-In Applets

| Applet | What it shows |
|---|---|
| [`audio`](#audio) | Volume, mute state, output device, microphone indicator. |
| [`battery`](#battery) | Battery state, charge, time left, power profile. |
| [`bluetooth`](#bluetooth) | Bluetooth state and connected devices. |
| [`brightness`](#brightness) | Screen brightness with scroll control. |
| [`clipboard`](#clipboard) | Clipboard history. |
| [`clock`](#clock) | Time, date, calendar, and optional world clocks. |
| [`command`](#command) | A button or menu that runs commands. |
| [`exec`](#exec) | A live custom status widget from your script. |
| [`keyboard`](#keyboard) | Current keyboard layout. |
| [`mpris`](#mpris) | Media players and playback controls. |
| [`network`](#network) | Wi-Fi, wired network, and VPN status. |
| [`notifications`](#notifications) | Notification center and popups. |
| [`pager`](#pager) | Windows of the panel monitor's active workspace (or workspace dots on Hyprland). |
| [`workspaces-pager`](#workspaces-pager) | Per-monitor workspaces with names. |
| [`privacy`](#privacy) | Camera, microphone, screen sharing, and location indicators. |
| [`removable`](#removable) | USB drives and removable storage. |
| [`session`](#session) | Lock, logout, suspend, restart, and shutdown. |
| [`tray`](#tray) | App tray icons. |
| [`weather`](#weather) | Current weather and forecast. |

## Audio

Shows the current output volume. Scroll on the applet to change volume.

```toml
[applets.audio]
label = "{volume}%"
tooltip = "{device} - {volume}%"
scroll_step = 5
max_volume = 120
show_mic_indicator = true
show_streams = true
```

| Option | Default | Meaning |
|---|---|---|
| `show_icon` | `true` | Show the volume icon. |
| `show_mic_indicator` | `true` | Show a microphone indicator when input is active. |
| `label` / `label_format` | `""` | Panel text. |
| `tooltip` / `tooltip_format` | `"{device} - {volume}%"` | Hover text. |
| `scroll_step` | `10` | Volume change per scroll step. |
| `max_volume` | `100` | Maximum volume allowed from scrolling and controls. |
| `show_streams` | `true` | Show application streams in the popover. |

Placeholders: `{state}`, `{volume}`, `{device}`, `{input_volume}`, `{input_device}`.

## Battery

Shows battery and charging state. The popover includes time left and power information when available.

```toml
[applets.battery]
show_icon = true
label_on_battery = "{percentage}%"
label_on_ac = ""
tooltip_on_battery = "{percentage}% {state}, {time_left}"
tooltip_on_ac = "{percentage}% {state}"
settings_command = "gnome-control-center power"
```

| Option | Default | Meaning |
|---|---|---|
| `show_icon` | `true` | Show the battery icon. |
| `label_on_battery` | `""` | Panel text while unplugged. |
| `label_on_ac` | `""` | Panel text while plugged in. |
| `tooltip_on_battery` | `"{percentage}% {state}, {time_left}"` | Hover text while unplugged. |
| `tooltip_on_ac` | `"{percentage}% {state}"` | Hover text while plugged in. |
| `settings_command` | `""` | Optional command for opening power settings. |

Placeholders: `{percentage}`, `{state}`, `{time_left}`.

## Bluetooth

Shows Bluetooth status and connected device count.

```toml
[applets.bluetooth]
label = "{devices}"
tooltip = "{devices} connected devices"
```

| Option | Default | Meaning |
|---|---|---|
| `label` / `label_format` | `""` | Panel text. |
| `tooltip` / `tooltip_format` | `"{devices} connected devices"` | Hover text. |

Placeholders: `{devices}`, `{state}`.

## Brightness

Shows brightness. Scroll on the applet to change brightness.

```toml
[applets.brightness]
label = "{percent}%"
tooltip = "{source}: {percent}%"
scroll_step = 5
```

| Option | Default | Meaning |
|---|---|---|
| `label` / `label_format` | `""` | Panel text. |
| `tooltip` / `tooltip_format` | `"{source}: {percent}%"` | Hover text. |
| `scroll_step` | `10` | Brightness change per scroll step. |

Placeholders: `{source}`, `{percent}`.

## Clipboard

Shows clipboard history and opens a popover for copying older entries.

```toml
[applets.clipboard]
label = "{count}"
tooltip = "{count} clipboard items"
show_when_empty = false
```

| Option | Default | Meaning |
|---|---|---|
| `label` / `label_format` | `""` | Panel text. |
| `tooltip` / `tooltip_format` | `"{count} clipboard items"` | Hover text. |
| `show_when_empty` | `false` | Keep the applet visible with no history. |

Placeholders: `{count}`, `{state}`.

## Clock

Shows time and opens a calendar popover.

```toml
[applets.clock]
format = "%H:%M"
tooltip = "%A, %B %-d %Y"
tick_interval = 1

[[applets.clock.timezones]]
label = "Tokyo"
timezone = "Asia/Tokyo"
```

| Option | Default | Meaning |
|---|---|---|
| `format` / `label_format` | `"%a %-d %b, %H:%M"` | Panel time format. |
| `tooltip` / `tooltip_format` | `"%A, %-d %B %Y"` | Hover date format. |
| `tick_interval` | `1` | Seconds between clock updates. Values are clamped from 1 to 60. |
| `timezones` | `[]` | Optional world clocks shown in the popover. |

Clock formats use `strftime` style patterns.

## Command

Runs a command from a panel button or menu.

```toml
[applets.terminal]
extends = "command"
icon = "utilities-terminal-symbolic"
label = "Terminal"
tooltip = "Open terminal"
command = ["ghostty"]
```

| Option | Default | Meaning |
|---|---|---|
| `extends` | required for custom names | Use `"command"` for custom command applets. |
| `icon` | unset | Symbolic icon name. |
| `label` | unset | Optional button text. |
| `tooltip` | unset | Hover text. |
| `command` | `[]` | Command run when clicked. |
| `menu` | `[]` | Optional menu items with `label` and `command`. |

Read the full [Command Applet](../custom-applets/command.md) guide for menus and shell examples.

## Exec

Runs your own script and lets it draw status items and popovers.

```toml
[applets.sysinfo]
extends = "exec"
command = ["sh", "-c", "~/.config/glimpse/scripts/sysinfo"]
restart_delay_ms = 1000

[applets.sysinfo.options]
interval = 5
```

| Option | Default | Meaning |
|---|---|---|
| `extends` | required for custom names | Use `"exec"` for custom exec applets. |
| `command` | `[]` | Script or program to run. Required. |
| `restart_delay_ms` | `1000` | Delay before restarting the script after it exits. |
| `options` | `{}` | Custom data sent to your script on startup. |

Read [Exec Applet](../custom-applets/exec.md) for basic usage and [Exec SDK](./exec-sdk.md) for components, events, and best practices.

## Keyboard

Shows the current keyboard layout.

```toml
[keyboard]
remember = "global"

[keyboard.labels]
"English (US)" = "EN"
"Polish" = "PL"

[applets.keyboard]
labels = { "English (US)" = "EN", "Polish" = "PL" }
```

| Option | Default | Meaning |
|---|---|---|
| `labels` | `{}` | Replace long layout names with short labels. |

You can set labels globally under `[keyboard.labels]` or directly on the applet.

## MPRIS

Shows media player status and playback controls.

```toml
[applets.mpris]
label = "{artist} - {title}"
tooltip = "{player}: {artist} - {title}"
hide_when_empty = true
max_rows = 5
show_artwork = true
```

| Option | Default | Meaning |
|---|---|---|
| `label` / `label_format` | `"{artist} - {title}"` | Panel text. |
| `tooltip` / `tooltip_format` | `"{player}: {artist} - {title}"` | Hover text. |
| `hide_when_empty` | `true` | Hide when no player is active. |
| `max_rows` | `5` | Maximum players shown in the popover. Clamped from 1 to 12. |
| `show_artwork` | `true` | Show album art when available. |

Placeholders: `{player}`, `{artist}`, `{title}`, `{track}`, `{album}`, `{state}`, `{position}`, `{duration}`, `{remaining}`.

## Network

Shows current network state and opens a popover for Wi-Fi, wired network, and VPN entries.

```toml
[applets.network]
label = "{network}"
tooltip = "{state}"
```

| Option | Default | Meaning |
|---|---|---|
| `label` / `label_format` | `""` | Panel text. |
| `tooltip` / `tooltip_format` | `"{state}"` | Hover text. |

Placeholders: `{state}`, `{network}`, `{type}`, `{wifi}`, `{access_points}`, `{connections}`, `{vpns}`, `{speed}`.

## Notifications

Shows notification state, a notification center, and popups.

```toml
[applets.notifications]
label = "{count}"
tooltip = "{count} notifications"
badge_style = "count"
popup_timeout_ms = 5000
popup_visible_limit = 8
popup_position = "top_center"
popup_margin_x = 12
popup_margin_y = 32
```

| Option | Default | Meaning |
|---|---|---|
| `label` / `label_format` | `""` | Panel text. |
| `tooltip` / `tooltip_format` | `"{count} notifications"` | Hover text. |
| `badge_style` | `"count"` | Badge style. Use `"none"` to hide the badge. |
| `popup_timeout_ms` | `5000` | How long popups stay visible. |
| `popup_visible_limit` | `8` | Maximum popups visible at once. |
| `popup_position` | `"top_center"` | Popup position. |
| `popup_margin_x` | `12` | Horizontal popup margin. |
| `popup_margin_y` | `32` | Vertical popup margin. |

Placeholders: `{count}`, `{state}`.

## Pager

Auto-detects what to show based on the compositor:

- **niri**: dots for the windows on the panel monitor's active workspace (so each monitor's panel shows its own windows, even when keyboard focus is on a different monitor); the focused window is highlighted, others use a medium tint, and an empty workspace renders a single placeholder dot.
- **Hyprland / unsupported**: workspace dots, with the focused workspace highlighted.

```toml
[applets.pager]
count = 10
scroll_action = "workspaces"
```

| Option | Default | Meaning |
|---|---|---|
| `count` | `10` | Number of workspace slots to show (Hyprland workspace mode only). |
| `scroll_action` | unset | What scrolling does: `"workspaces"` or `"windows"`. |

## Workspaces Pager

Shows the workspaces of the panel's own monitor as named dots. On niri each monitor's panel renders only its own workspaces, and only the workspace that holds keyboard focus globally gets the strong highlight — workspaces that are merely visible on an unfocused monitor stay dim. Workspace names (when set) replace the slot number inside the dot.

```toml
[applets.workspaces-pager]
count = 10
scroll_action = "workspaces"
```

| Option | Default | Meaning |
|---|---|---|
| `count` | `10` | Minimum number of workspace slots to show on Hyprland. niri uses the live workspace count for the panel's monitor and ignores this. |
| `scroll_action` | unset | What scrolling does: `"workspaces"` or `"windows"`. |

You can place both pagers in the same panel — for example a windows pager on the left and a workspaces pager on the right:

```toml
[panels.center]
left = ["workspaces-pager"]
right = ["pager"]
```

## Privacy

Shows privacy indicators such as microphone, camera, screen sharing, and location use.

```toml
[applets.privacy]
```

This applet has no user config today.

| Option | Default | Meaning |
|---|---|---|
| none | none | The privacy applet is configured by placing or removing it from the panel. |

## Removable

Shows USB drives and removable storage.

```toml
[applets.removable]
show_when_empty = false
label_format = "{mounted}/{count}"
tooltip_format = "{count} removable device(s), {mounted} mounted"
```

| Option | Default | Meaning |
|---|---|---|
| `show_when_empty` | `false` | Keep visible with no removable devices. |
| `label_format` | `""` | Panel text. |
| `tooltip_format` | `"{count} removable device(s), {mounted} mounted"` | Hover text. |

Placeholders: `{count}`, `{mounted}`.

## Session

Shows the current user and opens session actions.

```toml
[applets.session]
label = "{user}"
tooltip = "{user} on {host}"
show_lock = true
show_logout = true
show_suspend = true
show_hibernate = false
show_reboot = true
show_shutdown = true
confirm_logout = true
confirm_suspend = true
confirm_hibernate = true
confirm_reboot = true
confirm_shutdown = true
```

| Option | Default | Meaning |
|---|---|---|
| `label` / `label_format` | `"{user}"` | Panel text. |
| `tooltip` / `tooltip_format` | `"{user} on {host}"` | Hover text. |
| `show_lock` | `true` | Show lock action. |
| `show_logout` | `true` | Show logout action. |
| `show_suspend` | `true` | Show suspend action. |
| `show_hibernate` | `false` | Show hibernate action. |
| `show_reboot` | `true` | Show restart action. |
| `show_shutdown` | `true` | Show shutdown action. |
| `confirm_logout` | `true` | Ask before logout. |
| `confirm_suspend` | `true` | Ask before suspend. |
| `confirm_hibernate` | `true` | Ask before hibernate. |
| `confirm_reboot` | `true` | Ask before restart. |
| `confirm_shutdown` | `true` | Ask before shutdown. |

Placeholders: `{user}`, `{host}`, `{uptime}`, `{state}`.

## Tray

Shows app tray icons.

```toml
[applets.tray]
icon_size = 16
show_passive = false
```

| Option | Default | Meaning |
|---|---|---|
| `icon_size` | `16` | Tray icon size. Clamped from 12 to 32. |
| `show_passive` | `false` | Show passive tray items. |

## Weather

Shows current weather and a forecast popover.

```toml
[applets.weather]
city_name = "Warsaw, PL"
geolocate = false
hourly_slots = 5
forecast_days = 5
label = "{temp}°"
tooltip = "{condition} · {temp} · feels like {feels_like} · {location}"
refresh_interval = 1800
```

| Option | Default | Meaning |
|---|---|---|
| `city_name` | `""` | Place to use when not geolocating. |
| `geolocate` | `false` | Use configured location instead of `city_name`. |
| `hourly_slots` | `5` | Hourly entries in the popover. Clamped from 1 to 8. |
| `forecast_days` | `5` | Forecast days in the popover. Clamped from 1 to 10. |
| `label` / `label_format` | `"{temp}°"` | Panel text. |
| `tooltip` / `tooltip_format` | `"{condition} · {temp} · feels like {feels_like} · {location}"` | Hover text. |
| `refresh_interval` | `1800` | Seconds between updates. Minimum 60. |

Placeholders: `{temp}`, `{condition}`, `{feels_like}`, `{location}`.

To use your shared location:

```toml
[location]
provider = "static"
latitude = 52.2297
longitude = 21.0122

[applets.weather]
geolocate = true
```
