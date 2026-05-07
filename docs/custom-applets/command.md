# Command Applet

The command applet is the easiest way to add a launcher or small menu to your panel.

## Button Example

```toml
[applets.terminal]
extends = "command"
icon = "utilities-terminal-symbolic"
label = "Terminal"
tooltip = "Open terminal"
command = ["ghostty"]
```

Add it to a panel:

```toml
right = ["terminal", "network", "battery"]
```

## Menu Example

```toml
[applets.power-menu]
extends = "command"
icon = "system-shutdown-symbolic"
tooltip = "Power"
command = ["loginctl", "lock-session"]

[[applets.power-menu.menu]]
label = "Suspend"
command = ["systemctl", "suspend"]

[[applets.power-menu.menu]]
label = "Restart"
command = ["systemctl", "reboot"]

[[applets.power-menu.menu]]
label = "Shutdown"
command = ["systemctl", "poweroff"]
```

The main button runs `command`. The menu items run their own commands.

## Options

| Option | What it does |
|---|---|
| `icon` | Symbolic icon name. |
| `label` | Optional text beside the icon. |
| `tooltip` | Text shown on hover. |
| `command` | Command to run when clicked. |
| `menu` | Optional right-click or popover menu items. |

## Shell Examples

Open a web app:

```toml
command = ["xdg-open", "https://calendar.google.com"]
```

Use shell features:

```toml
command = ["sh", "-c", "grim ~/Pictures/Screenshots/$(date +%F-%H%M%S).png"]
```

When in doubt, write the command exactly as you would run it in a terminal, then wrap it with `["sh", "-c", "..."]` if it needs shell syntax.
