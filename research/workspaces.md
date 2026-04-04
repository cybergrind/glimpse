# Workspaces Provider

**Source:** Compositor IPC (niri, hyprland)

**What it does:** Lists workspaces, reports active/focused workspace, switches workspaces, and tracks workspace create/destroy events. Entirely compositor-specific.

## System Interface

### Niri IPC

#### Query workspaces (`niri msg --json workspaces`)

Returns JSON array of workspace objects:

```json
[
  {
    "id": 1,
    "idx": 1,
    "name": null,
    "output": "eDP-1",
    "is_urgent": false,
    "is_active": true,
    "is_focused": true,
    "active_window_id": 42
  }
]
```

Fields:
- `id: u64` — stable unique ID (persists across reorder/monitor moves)
- `idx: u8` — index on its monitor (1-based)
- `name: Option<String>` — display name (null if unnamed/dynamic)
- `output: Option<String>` — monitor name (e.g. "eDP-1")
- `is_urgent: bool` — contains an urgent window
- `is_active: bool` — active on its monitor
- `is_focused: bool` — currently focused (keyboard input goes here)
- `active_window_id: Option<u64>` — focused window in this workspace

#### Switch workspace

- `niri msg action focus-workspace INDEX` — by index on current monitor
- `niri msg action focus-workspace NAME` — by name (named workspaces only)

Reference types: `Index(u8)` or `Name(String)`.

#### Move window

- `niri msg action move-window-to-workspace INDEX`
- `niri msg action move-window-to-workspace NAME`
- Add `--focus=false` to not follow the window

#### Named workspaces

Declared in niri config — persistent, never auto-deleted even when empty. Dynamic workspaces are created on-demand and deleted when empty.

#### Workspace model

Niri uses a **scrolling/infinite strip** model:
- Each monitor has an independent vertical stack of workspaces
- Workspaces are per-monitor (not global)
- New empty workspace always available at the bottom
- Windows arranged in columns on a horizontal scrollable strip within each workspace

#### Event subscription

`niri msg event-stream` — persistent connection, newline-delimited JSON. Sends full initial state, then incremental updates for workspace changes.

Socket: `$NIRI_SOCKET`

### Hyprland IPC

#### Query workspaces (`hyprctl workspaces -j`)

Returns JSON array:

```json
[
  {
    "id": 1,
    "name": "1",
    "active": false,
    "class": "workspace-button w1"
  },
  {
    "id": 4,
    "name": "4",
    "active": true,
    "class": "workspace-button w4 workspace-active wa4"
  }
]
```

Fields:
- `id: i32` — numeric workspace ID (1–2147483647)
- `name: String` — workspace name (defaults to string of ID)
- `active: bool`
- `class: String` — CSS class string for styling

Workspace IDs can be non-sequential (e.g. 1, 2, 4 — workspace 3 doesn't exist).

#### Active workspace per monitor

From `hyprctl monitors -j`, each monitor has:
```json
"activeWorkspace": { "id": 4, "name": "4" }
```

#### Switch workspace

- `hyprctl dispatch workspace N` — absolute (1–2147483647)
- `hyprctl dispatch workspace +1` / `-1` — relative
- `hyprctl dispatch workspace m+1` / `m-1` — relative on current monitor
- `hyprctl dispatch workspace e+1` / `e-1` — skip empty workspaces
- `hyprctl dispatch workspace name:Web` — by name

#### Special workspaces (scratchpads)

Toggleable overlay workspaces, max 97 at a time.

- `hyprctl dispatch togglespecialworkspace NAME` — show/hide
- Named with `special:NAME` prefix in events
- Can assign windows via rules: `workspace = special:scratchpad`

#### Workspace model

Hyprland uses a **traditional tiling** model:
- Global workspace pool shared across monitors
- One active workspace per monitor at a time
- Workspaces are dynamic by default (created/destroyed as needed)
- Can be made persistent via workspace rules

#### Event socket

Line-delimited events on `.socket2.sock`:

- `workspace>>ID` — active workspace changed
- `createworkspace>>NAME` — workspace created (e.g. `createworkspace>>5`, `createworkspace>>special:1`)
- `destroyworkspace>>NAME` — workspace deleted
- `focusedmon>>MONITORNAME,WORKSPACENAME` — monitor focus changed

Command socket: `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`
Event socket: `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`

## Topics

- `workspaces.list` — all workspaces with active/focused/urgent state
- `workspaces.active` — currently focused workspace

## Methods

- `workspaces.switch(reference: WorkspaceReference)` — switch to workspace by ID, index, or name
- `workspaces.move_window(reference: WorkspaceReference, follow_focus: bool)` — move focused window to workspace

## Types

```rust
/// How to reference a workspace
enum WorkspaceReference {
    /// By numeric ID (Hyprland) or stable ID (niri)
    Id(u64),
    /// By index on current monitor
    Index(u32),
    /// By name
    Name(String),
    /// Relative: next/previous
    Next,
    Previous,
}

/// A workspace
struct Workspace {
    /// Unique identifier
    id: u64,
    /// Display name
    name: Option<String>,
    /// Index on its monitor (1-based)
    index: u32,
    /// Which monitor this workspace is on
    output: Option<String>,
    /// Currently active on its monitor
    is_active: bool,
    /// Currently focused (receives keyboard input)
    is_focused: bool,
    /// Contains an urgent window
    is_urgent: bool,
    /// Whether this is a special/scratchpad workspace (Hyprland)
    is_special: bool,
}

/// Emitted on `workspaces.list`
struct WorkspaceList {
    workspaces: Vec<Workspace>,
}
```

## Icons

- `view-grid-symbolic` — workspace grid/overview
- `preferences-desktop-workspaces-symbolic` — workspace settings (if available)

## Crates

- `niri-ipc` — niri IPC bindings (typed `Workspace`, `KeyboardLayouts`, `WorkspaceReferenceArg`)
- `hyprland` (0.4) — Hyprland IPC wrapper (async, typed)

## Change Detection

**Niri:** `EventStream` IPC — sends full initial state then incremental workspace updates. Fully reactive, no polling needed.

**Hyprland:** Event socket events:
- `workspace>>ID` — focus changed
- `createworkspace>>NAME` — created
- `destroyworkspace>>NAME` — deleted
- `focusedmon>>MONITOR,WORKSPACE` — monitor focus changed

Hyprland does not send full state on connection — must poll `hyprctl workspaces -j` for initial state, then apply events incrementally.

## Features

- List all workspaces with active/focused/urgent state
- Report active workspace per monitor
- Switch workspace by ID, index, name, or relative (next/prev)
- Move window to workspace with optional focus follow
- Named/persistent workspace support
- Dynamic workspace create/destroy tracking
- Special/scratchpad workspace support (Hyprland)
- Per-monitor workspace tracking (niri)
- Urgent workspace notification
- Compositor auto-detection (niri vs hyprland)

## Notes

- Entirely compositor-specific — no universal Wayland protocol for workspaces
- Niri workspaces are per-monitor; Hyprland workspaces are global
- Niri uses stable IDs that persist across reorder; Hyprland uses sequential integers
- Hyprland workspace IDs can have gaps (e.g. 1, 2, 5)
- Special workspaces in Hyprland use `special:` prefix — filter them separately in UI
- Niri event stream provides full state first, preventing desync; Hyprland requires initial poll
