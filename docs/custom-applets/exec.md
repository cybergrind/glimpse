# Exec Applet

The exec applet is for live custom status. Your script decides what the panel shows.

Use it for things like:

| Idea | Example |
|---|---|
| System status | CPU temperature, memory, disk space. |
| Personal workflow | Current task, timer, VPN status. |
| Web data | Build status, unread count, package updates. |
| Setup details | Current theme, song mood, workspace mode. |

## Basic Config

```toml
[applets.sysinfo]
extends = "exec"
command = ["sh", "-c", "~/.config/glimpse/scripts/sysinfo"]
restart_delay_ms = 1000

[applets.sysinfo.options]
unit = "celsius"
```

Add it to the panel:

```toml
right = ["sysinfo", "network", "battery"]
```

## What Your Script Prints

Your script prints lines to update the applet.

Set the visible status:

```txt
status {"items":[{"id":"cpu","label":"42°C","icon":{"name":"temperature-symbolic"},"tooltip":"CPU temperature"}]}
```

Set popover content:

```txt
popover {"root":{"type":"detail_grid","data":{"rows":[{"key":"CPU","value":"42°C"},{"key":"RAM","value":"51%"}]}}}
```

Keep the script running if the value changes over time.

## Click Events

When the user clicks an item with an `id`, your script receives an event line:

```txt
event {"id":"cpu","type":"click","source":"status","button":"left"}
```

That lets your script react to clicks, open tools, or change what it shows.

## Practical Script Shape

```sh
#!/bin/sh

while true; do
  temp="$(sensors | rg 'Package id 0' | rg -o '[0-9]+\\.[0-9]+°C' | head -n1)"
  printf 'status {"items":[{"id":"cpu","label":"%s","icon":{"name":"temperature-symbolic"}}]}\n' "$temp"
  sleep 5
done
```

Make it executable:

```sh
chmod +x ~/.config/glimpse/scripts/sysinfo
```

## Tips

| Tip | Why |
|---|---|
| Print one status line at startup | The panel has something to show immediately. |
| Keep updates modest | Once every few seconds is enough for most status items. |
| Restart safely | If the script exits, Glimpse starts it again after `restart_delay_ms`. |
| Use `sh -c` for shell scripts | It keeps paths, pipes, and redirects predictable. |

For popover components, events, and complete protocol details, read the [Exec SDK](../applets/exec-sdk.md).
