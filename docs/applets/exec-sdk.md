# Exec SDK

The exec applet is a small line-based interface for scripts. Your script prints JSON lines to control the panel, and Glimpse sends JSON lines back when the user clicks, scrolls, opens a popover, or changes a control.

You can write exec applets in shell, Python, Go, Node, Rust, or any program that can read standard input and write standard output.

## Applet Config

```toml
[applets.sysinfo]
extends = "exec"
command = ["sh", "-c", "~/.config/glimpse/scripts/sysinfo"]
restart_delay_ms = 1000

[applets.sysinfo.options]
interval = 5
unit = "celsius"
```

| Option | Default | Meaning |
|---|---|---|
| `extends` | required for custom names | Use `"exec"` for custom exec applets. |
| `command` | `[]` | Program to run. Required. Use `["sh", "-c", "..."]` when you need shell syntax. |
| `restart_delay_ms` | `1000` | Delay before the program restarts after exit. |
| `options` | `{}` | Your own settings. Glimpse sends them to the script in the `init` message. |

## Line Protocol

Your script sends:

```txt
status {"items":[{"id":"cpu","label":"42%","icon":{"name":"cpu-symbolic"},"tooltip":"CPU usage"}]}
popover {"root":{"type":"section","data":{"title":"System","children":[]}}}
```

Glimpse sends:

```txt
init {"instance":"sysinfo","options":{"interval":5,"unit":"celsius"}}
event {"id":"cpu","type":"click","source":"status","button":"left"}
```

Each line starts with a command name, a space, and a JSON object.

## Status Items

Status items are shown directly in the panel.

```txt
status {"items":[
  {"id":"cpu","icon":{"name":"cpu-symbolic"},"label":"12%","tooltip":"CPU"},
  {"id":"mem","icon":{"name":"memory-symbolic"},"label":"51%","tooltip":"Memory"}
]}
```

| Field | Default | Meaning |
|---|---|---|
| `id` | unset | Optional event id. Add it if you want clicks or scrolls. |
| `icon` | unset | Optional icon, either `{"name":"icon-name"}` or `{"path":"/path/to/image.png"}`. |
| `label` | unset | Optional text in the panel. |
| `tooltip` | unset | Optional hover text. |

Left-click opens the popover when the applet has popover content. Right-click opens the context menu if available.

## Popover Root

Popover content is a tree:

```txt
popover {"root":{"type":"section","data":{
  "title":"System",
  "body":[
    {"type":"item","data":{"label":"CPU","right":{"type":"badge","data":{"label":"42%"}}}},
    {"type":"meter","data":{"label":"Memory","value":0.51,"text":"51%"}}
  ]
}}}
```

The `type` chooses the component. The `data` object contains the component fields.

## Common Component Fields

Most popover components accept these fields:

| Field | Default | Values | Meaning |
|---|---|---|---|
| `id` | unset | string | Required for interactive components. Used in events. |
| `visible` | unset, treated as visible | boolean | Hide or show the component. |
| `hexpand` / `vexpand` | unset, treated as `false` | boolean | Let the component take extra space. |
| `halign` / `valign` | unset | `fill`, `start`, `end`, `center`, `baseline` | Alignment. |
| `tooltip` | unset | string | Hover text. |
| `variant` | unset, treated as `normal` | `normal`, `muted`, `accent`, `success`, `warning`, `danger` | Visual emphasis. |

## Layout Components

| Component | Default fields | Use it for |
|---|---|---|
| `section` | `title = unset`, `subtitle = ""`, `header = unset`, `body = []`, `children = []` | A titled group. |
| `collapsible` | `title = unset`, `subtitle = ""`, `expanded = false`, `body = []`, `children = []` | Expandable group. |
| `card` | `children = []` | A framed group. |
| `row` | `spacing = 0`, `children = []` | Horizontal layout. |
| `column` | `spacing = 0`, `children = []` | Vertical layout. |
| `box` | `spacing = 0`, `children = []` | Explicit horizontal or vertical layout. Requires `orientation`. |
| `grid` | `row_spacing = 0`, `column_spacing = 0`, `children = []` | Two-dimensional layout. |
| `scroll` | no default child | Scrollable content. Requires `child`. |
| `separator` | `orientation = unset` | Visual divider. |

Grid children use:

```json
{"row":0,"column":0,"width":1,"height":1,"child":{"type":"label","data":{"text":"CPU"}}}
```

## Display Components

| Component | Default fields | Use it for |
|---|---|---|
| `hero` | `subtitle = ""`, `icon = unset`; requires `title` | Big header for a popover. |
| `item` | `left = unset`, `label = ""`, `right = unset`, `clickable = false` | Standard list row. |
| `collapsible_item` | `left = unset`, `label = ""`, `right = unset`, `expanded = false`, `body = []`, `children = []` | Expandable list row. |
| `action_row` | `subtitle = ""`, `meta = ""`, `icon = unset`; requires `title` | Clickable-looking row with summary text. |
| `action_menu` | `header = unset`, `items = []` | Menu of script-defined actions. |
| `detail_grid` | `rows = []` | Key/value facts. |
| `empty_state` | `subtitle = ""`; requires `title` | Friendly empty message. |
| `badge` | requires `label` | Small pill label. |
| `status` | common fields only | Small status marker. |
| `meter` | `icon = unset`, `label = ""`, `min = 0`, `max = 1`, `step = 0.01`, `text = unset`, `interactive = false`; requires `value` | Progress row or slider row. |
| `progress` | `max = 1`, `show_text = false`, `text = unset`; requires `value` | Progress bar. |
| `copyable` | `label = ""`; requires `value` | Text row with copy action. |
| `toast` | `icon = unset`, `message = ""`, `action = unset`; requires `title` | Inline notice. |
| `spinner` | `spinning = true` | Loading indicator. |
| `label` | `wrap = false`, `xalign = unset`, `selectable = false`; requires `text` | Text. |
| `icon` | `pixel_size = unset`; requires `icon` | Symbolic icon. |
| `image` | `pixel_size = unset`; requires `icon` | Image from icon name or path. |
| `button` | `label = unset`, `icon = unset`, `child = unset`; requires `id` for events | Button. |
| `switch` | `label = unset`, `active = false`; requires `id` | Toggle switch. |
| `checkbox` | `label = unset`, `active = false`; requires `id` | Checkbox. |
| `scale` | `orientation = unset`, `draw_value = false`; requires `id`, `min`, `max`, `step`, `value` | Slider. |
| `dropdown` | `items = []`, `selected = unset`; requires `id` | Dropdown. |

## Interactive Components

These components send events back to your script.

| Component | Event | Payload |
|---|---|---|
| Status item with `id` | `click`, `scroll` | `button` or `delta_y`. |
| `button` | `click` | `button = "left"`. |
| `item` with `clickable = true` and `id` | `click` | `button = "left"`. |
| `action_menu` item | `click` | selected item id. |
| `switch` | `toggle` | `active = true` or `false`. |
| `checkbox` | `toggle` | `active = true` or `false`. |
| `scale` | `change` | numeric `value`. |
| interactive `meter` | `change` | numeric `value`. |
| `dropdown` | `change` | selected item id, label, and index. |
| Popover | `open`, `close` | id `popover`. |

Example event:

```txt
event {"id":"volume","type":"change","source":"popover","value":0.72}
```

## Action Menu

Use `action_menu` when a small list of actions is better than separate buttons.

```txt
popover {"root":{"type":"action_menu","data":{
  "header":"Power profile",
  "items":[
    {"id":"power-saver","label":"Power Saver","checked":false},
    {"id":"balanced","label":"Balanced","checked":true},
    {"id":"performance","label":"Performance","checked":false}
  ]
}}}
```

## Shell Starter

```sh
#!/bin/sh

printf 'status {"items":[{"id":"load","label":"starting","icon":{"name":"utilities-system-monitor-symbolic"}}]}\n'

while IFS= read -r line; do
  case "$line" in
    init\ *)
      printf 'status {"items":[{"id":"load","label":"ready","icon":{"name":"utilities-system-monitor-symbolic"}}]}\n'
      ;;
    event\ *)
      printf 'popover {"root":{"type":"section","data":{"title":"System","body":[{"type":"item","data":{"label":"Last event","right":{"type":"badge","data":{"label":"seen"}}}}]}}}\n'
      ;;
  esac
done
```

This shape is event-driven. For polling, run a background loop and keep reading events in the foreground.

## How-To: CPU Temperature

```sh
#!/bin/sh

while true; do
  temp="$(sensors | rg 'Package id 0' | rg -o '[0-9]+\\.[0-9]+°C' | head -n1)"
  [ -n "$temp" ] || temp="n/a"
  printf 'status {"items":[{"id":"cpu","icon":{"name":"temperature-symbolic"},"label":"%s","tooltip":"CPU temperature"}]}\n' "$temp"
  sleep 5
done
```

Config:

```toml
[applets.cpu-temp]
extends = "exec"
command = ["sh", "-c", "~/.config/glimpse/scripts/cpu-temp"]
```

## How-To: Toggle A Command

Use a button and handle its click event:

```txt
popover {"root":{"type":"section","data":{"title":"VPN","body":[{"type":"button","data":{"id":"toggle-vpn","label":"Toggle VPN","variant":"accent"}}]}}}
```

Your script receives:

```txt
event {"id":"toggle-vpn","type":"click","source":"popover","button":"left"}
```

Then run your command and print updated `status` and `popover` lines.

## Best Practices

| Practice | Why |
|---|---|
| Print status immediately | The panel should not sit empty while your script warms up. |
| Keep panel labels short | Long labels make the panel jump and crowd other applets. |
| Put detail in the popover | The panel is for glanceable state; popovers are for explanations and controls. |
| Use stable ids | Events are easier to handle when ids do not change between updates. |
| Throttle polling | Most system stats do not need sub-second updates. |
| Send complete updates | Treat each `status` or `popover` line as the current truth. |
| Use variants sparingly | `warning` and `danger` should mean something needs attention. |
| Validate JSON before running | Bad JSON is ignored and logged. |
| Keep stderr quiet | Use stderr for useful diagnostics, not a constant stream. |
| Prefer one script per concern | A small CPU applet is easier to maintain than one giant script for everything. |

## When To Use Exec

Use `exec` for personal, local, and fast-changing desktop widgets. If you need a permanent applet that many users will depend on, start with `exec`, learn the shape, then consider making it a built-in applet later.
