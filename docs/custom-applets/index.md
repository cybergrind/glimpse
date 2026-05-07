# Custom Applets

Custom applets let you add your own buttons, menus, and live status items to the panel.

Use them when the built-in applets are not enough, or when you want your desktop to show exactly the things you care about.

## Two Types

| Type | Use it for |
|---|---|
| [`command`](./command.md) | A button or menu that runs commands. |
| [`exec`](./exec.md) | A script that continuously controls what the applet shows. |

## Quick Launcher

```toml
[applets.terminal]
extends = "command"
icon = "utilities-terminal-symbolic"
tooltip = "Open terminal"
command = ["ghostty"]

[[panels]]
right = ["terminal", "network", "battery"]
```

## Shell Syntax

Commands are always explicit. If you want shell features such as pipes, redirects, `~`, variables, or `&&`, run a shell yourself:

```toml
[applets.note-time]
extends = "command"
icon = "document-edit-symbolic"
command = ["sh", "-c", "date >> ~/.cache/glimpse-notes.log"]
```

This keeps simple commands simple and makes complex commands clear.

## Which One Should I Use?

| You want | Choose |
|---|---|
| Open an app | `command` |
| Run one command on click | `command` |
| Show a small menu | `command` |
| Show changing text or icons | `exec` |
| React to clicks inside a custom status widget | `exec` |
| Build a mini applet with your own script | `exec` |
