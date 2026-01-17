# Glimpse Panel Applet Architecture

## Overview

Multi-panel Wayland status bar with pluggable applet system. Built-in applets + external applets via Unix socket + JSON protocol. Full panel IPC for external control.

## Config Structure (panel.toml)

```toml
[global]
socket_path = "/run/user/1000/glimpse-panel.sock"
control_socket_path = "/run/user/1000/glimpse-panel-ctl.sock"

[[panels]]
id = "main"
output = "DP-1"           # Wayland output name, "*" for all
position = "bottom"       # "top" | "bottom"
height = 36
left = ["workspaces", "window-title"]
center = ["clock-main"]
right = ["applet-host", "systray", "volume", "wifi", "bluetooth"]

[[panels]]
id = "laptop"
output = "eDP-1"
position = "top"
height = 32
left = ["workspaces"]
center = []
right = ["applet-host-laptop", "battery", "clock-secondary"]

# Built-in applet instances
[applets.clock-main]
type = "clock"
format = "%H:%M"

[applets.clock-secondary]
type = "clock"
format = "%I:%M %p"

# Applet host - displays all connected external applets
[applets.applet-host]
type = "applet-host"
show_disconnected = true

[applets.applet-host-laptop]
type = "applet-host"
show_disconnected = false
```

## Module Structure

```
glimpse-panel/src/
├── main.rs
├── app.rs                    # Top-level coordinator
├── config.rs                 # Config types (expanded)
├── panel/
│   ├── mod.rs
│   ├── component.rs          # Panel relm4 component
│   └── layout.rs             # Left/center/right boxes
├── applet/
│   ├── mod.rs                # Applet trait + types
│   ├── registry.rs           # Type -> Factory mapping
│   ├── slot.rs               # AppletSlot wrapper component
│   ├── content.rs            # Icon, Text, Composite (for external)
│   ├── popover.rs            # Structured popover schema (for external)
│   └── menu.rs               # Context menu types
├── applets/                  # Built-in implementations (full GTK access)
│   ├── mod.rs
│   ├── clock.rs
│   ├── applet_host.rs        # Hosts external applets
│   ├── volume.rs
│   ├── wifi.rs
│   ├── bluetooth.rs
│   ├── battery.rs
│   └── workspaces.rs
├── external/
│   ├── mod.rs
│   ├── server.rs             # Unix socket server for applets
│   ├── protocol.rs           # JSON message types
│   └── applet.rs             # ExternalApplet impl
└── ipc/
    ├── mod.rs
    ├── server.rs             # Control socket server
    └── protocol.rs           # Control commands & queries
```

## Built-in vs External Applets

### Built-in Applets (Full GTK Access)

Built-in applets are relm4 components with **full GTK access**. They can:
- Render arbitrary GTK widgets directly
- Use custom popovers with any GTK content
- Access system APIs directly (D-Bus, PulseAudio, etc.)
- Use any GTK/libadwaita widget in their UI

```rust
// Built-in applet as relm4 component
#[relm4::component(pub)]
impl SimpleComponent for ClockApplet {
    type Init = ClockConfig;
    type Input = ClockInput;
    type Output = AppletOutput;

    view! {
        gtk::Box {
            gtk::Label {
                #[watch]
                set_label: &model.time_string,
                add_css_class: "clock-label",
            }
        }
    }

    // Full GTK popover with any widgets
    fn popover_widget(&self) -> gtk::Widget {
        let calendar = gtk::Calendar::new();
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 8);
        box_.append(&calendar);
        // Can add any GTK widgets here
        box_.into()
    }
}
```

### External Applets (Structured Schema)

External applets communicate via JSON and use **structured content schema**:
- Content: Icon, Text, IconText, Composite
- Popover: Box, Label, Button, Slider, Toggle, List, Separator, Image
- Menu: Hierarchical menu items

Panel renders the schema using GTK widgets, ensuring consistent styling.

```json
// External applet content
{"type": "update_content", "content": {"type": "icon_text", "icon": "weather-clear", "label": "72°F"}}

// External applet popover (structured)
{"type": "show_popover", "schema": {
  "content": {
    "type": "box",
    "orientation": "vertical",
    "children": [
      {"type": "label", "text": "Weather"},
      {"type": "button", "id": "refresh", "label": "Refresh"}
    ]
  }
}}
```

## Core Traits

### BuiltinApplet (for built-in)

```rust
pub trait BuiltinApplet {
    type Config: DeserializeOwned;

    /// Create the panel widget (icon, text, etc.)
    fn panel_widget(&self) -> gtk::Widget;

    /// Create popover content (full GTK access)
    fn popover_widget(&self) -> Option<gtk::Widget> { None }

    /// Create context menu
    fn context_menu(&self) -> Option<gtk::PopoverMenu> { None }

    /// Tooltip text
    fn tooltip(&self) -> Option<String> { None }
}
```

### ExternalApplet (for external)

```rust
pub struct ExternalApplet {
    name: String,
    connection_id: u64,
    content: AppletContent,           // Structured
    tooltip: Option<TooltipContent>,  // Structured
    popover_schema: Option<PopoverSchema>,  // Structured
    menu_items: Vec<MenuItem>,        // Structured
    sender: mpsc::Sender<PanelMessage>,
}
```

## Applet Host (Built-in)

Special built-in applet that dynamically displays all connected external applets:

```rust
pub struct AppletHost {
    show_disconnected: bool,
    external_applets: HashMap<String, ExternalAppletState>,
}

impl BuiltinApplet for AppletHost {
    fn panel_widget(&self) -> gtk::Widget {
        let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        for (name, state) in &self.external_applets {
            if state.connected || self.show_disconnected {
                let widget = self.render_external_content(&state.content);
                box_.append(&widget);
            }
        }
        box_.into()
    }
}
```

## External Applet Protocol (JSON over Unix Socket)

**Applet → Panel:**
```json
{"type": "register", "name": "my-applet", "version": "1.0", "tick_interval_ms": 1000}
{"type": "update_content", "content": {"type": "icon_text", "icon": "weather-clear", "label": "72°F"}}
{"type": "update_tooltip", "tooltip": {"type": "text", "content": "Sunny"}}
{"type": "show_popover", "schema": {...}}
{"type": "show_menu", "items": [...]}
{"type": "close_popover"}
```

**Panel → Applet:**
```json
{"type": "registered", "instance_id": "my-applet", "config": {...}}
{"type": "event", "event": {"type": "clicked", "button": "left"}}
{"type": "event", "event": {"type": "popover_action", "action_id": "run-btn", "value": null}}
{"type": "event", "event": {"type": "menu_item_activated", "item_id": "settings"}}
{"type": "tick"}
{"type": "detached"}
```

**Action Flow:** Panel sends event → Applet receives → Applet executes action (shell script, API call, etc.) → Applet updates content

## Panel IPC (Control Socket)

Separate socket for external tools to control the panel.

**Control Commands:**
```json
{"type": "reload_config"}
{"type": "reload_css"}
{"type": "quit"}
{"type": "show_panel", "panel_id": "main"}
{"type": "hide_panel", "panel_id": "main"}
{"type": "toggle_panel", "panel_id": "main"}
{"type": "send_to_applet", "applet_name": "my-applet", "message": {...}}
```

**Query Commands:**
```json
{"type": "list_panels"}
{"type": "get_panel", "panel_id": "main"}
{"type": "list_applets"}
{"type": "get_applet", "name": "my-applet"}
{"type": "list_external_applets"}
{"type": "get_config"}
```

**CLI Tool (glimpse-ctl):**
```bash
glimpse-ctl reload           # Reload config
glimpse-ctl list-panels      # List panels
glimpse-ctl list-applets     # List applets
glimpse-ctl send my-applet '{"action": "refresh"}'
glimpse-ctl hide main        # Hide panel
glimpse-ctl quit             # Quit panel
```

## Component Hierarchy (relm4)

```
App
├── OutputMonitor (tracks Wayland outputs)
├── ExternalAppletServer (applet socket)
├── ControlServer (control socket)
├── AppletRegistry (type factories)
└── Panel[] (one per output)
    ├── LeftBox → AppletSlot[]
    ├── CenterBox → AppletSlot[]
    └── RightBox → AppletSlot[]

AppletSlot
├── BuiltinApplet (full GTK) OR ExternalApplet (structured)
├── Tooltip
├── Popover (GTK widget or rendered schema)
└── ContextMenu

AppletHost (built-in)
└── ExternalAppletSlot[] (dynamic, rendered from schema)
```

## Structured Content Types (External Applets)

```rust
pub enum AppletContent {
    Icon { name: String, size: i32 },
    Text { label: String, css_classes: Vec<String> },
    IconText { icon: String, label: String },
    Composite(Vec<AppletContent>),
    Empty,
}

pub enum PopoverContent {
    Box { orientation, spacing, children },
    Label { id, text, css_classes },
    Button { id, label },
    Slider { id, min, max, value, step },
    Toggle { id, label, active },
    List { id, items, selectable },
    Separator,
    Image { icon, size },
}
```

## Runtime Attachment Flow

1. External process connects to applet socket
2. Sends `register` with name
3. Server notifies all `applet-host` instances
4. Host adds applet to its display
5. Server sends `registered` to applet
6. Applet sends `update_content`
7. Host re-renders with new content

**On disconnect:** Host shows placeholder or removes (based on config)
**On reconnect:** Applet re-registers, host restores

## Implementation Order

1. **Phase 1: Config & Multi-Panel**
   - Expand config.rs with full schema
   - Create Panel component
   - Support multiple panels on different outputs

2. **Phase 2: Applet Framework**
   - BuiltinApplet trait
   - AppletSlot component
   - AppletRegistry

3. **Phase 3: External Applet Server**
   - Unix socket server
   - JSON protocol
   - ExternalApplet wrapper
   - Content schema renderer

4. **Phase 4: Applet Host**
   - AppletHost built-in applet
   - Dynamic external applet display
   - Routing clicks to correct applet

5. **Phase 5: Panel IPC**
   - Control socket server
   - Command handlers
   - glimpse-ctl CLI tool

6. **Phase 6: Built-in Applets**
   - Clock (simplest, good test)
   - Volume, Wifi, Bluetooth (D-Bus)
   - Battery, Workspaces

7. **Phase 7: Popover & Menu**
   - Schema renderer for external
   - Full GTK for built-in
   - Context menu
   - Click-outside handling

## New Dependencies

```toml
serde_json = "1"
chrono = "0.4"
zbus = "4"              # D-Bus for system services
```

## Files to Modify/Create

- `config.rs` - Expand with PanelConfig, AppletConfig
- `app.rs` - Multi-panel coordination, IPC handling
- `panel/component.rs` - Panel relm4 component
- `applet/mod.rs` - BuiltinApplet trait, types
- `applet/slot.rs` - AppletSlot component
- `applet/schema_renderer.rs` - Renders structured content to GTK
- `applets/applet_host.rs` - AppletHost implementation
- `external/server.rs` - Applet socket server
- `external/protocol.rs` - Applet JSON types
- `ipc/server.rs` - Control socket server
- `ipc/protocol.rs` - Control JSON types

## Design Decisions

- **Config format:** TOML
- **UI toolkit:** GTK4 + libadwaita
- **Socket paths:** Automatic from `$XDG_RUNTIME_DIR/glimpse-panel.sock` (not configurable)
- **External applet ordering:** Connection order (first connected first)
- **State persistence:** None (fresh start each restart)
- **Error handling:** Handle as encountered during implementation
- **Built-in applets:** Full GTK access
- **External applets:** Structured JSON schema

## Verification

1. Create `panel.toml` with two panels, each with `applet-host`
2. Run `cargo run` - verify both panels appear
3. Add clock applet - verify time displays (full GTK calendar popover)
4. Create simple external applet script - verify it appears in applet-host
5. Run `glimpse-ctl list-applets` - verify it shows connected applet
6. Click external applet - verify event sent to applet process
7. Run `glimpse-ctl reload` - verify config reloads
8. Disconnect external applet - verify host shows placeholder or removes
9. Edit config - verify hot-reload works
