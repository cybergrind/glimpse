# Keyboard Provider

**Source:** Compositor IPC (niri, hyprland), input method D-Bus (fcitx5, ibus)

**What it does:** Reports current keyboard layout, switches between configured layouts, tracks Caps Lock/Num Lock state, and integrates with input method frameworks.

## System Interface

### Niri IPC

#### Query layouts (`niri msg --json keyboard-layouts`)

```json
{
  "names": ["us", "ro", "de"],
  "current_idx": 1
}
```

Fields:
- `names: Vec<String>` — XKB layout names
- `current_idx: u8` — index of active layout in the array

#### Switch layout

- `niri msg action switch-layout "next"` — next layout
- `niri msg action switch-layout "prev"` — previous layout

No way to switch to a specific index via IPC (only next/prev).

#### Event subscription

Send `Request::EventStream` on the niri socket → receive continuous `Event` messages including layout changes.

#### Caps Lock / Num Lock

Not exposed via niri IPC.

Socket: `$NIRI_SOCKET`

### Hyprland IPC

#### Query layouts (`hyprctl devices -j`)

```json
{
  "keyboards": [
    {
      "name": "at-translated-set-2-keyboard",
      "active_layout_index": 1,
      "active_keymap": "Romanian",
      "capsLock": false,
      "numLock": false,
      "main": true
    }
  ]
}
```

Fields per keyboard:
- `name: String` — device identifier (may contain commas)
- `active_layout_index: u32` — index of active layout
- `active_keymap: String` — human-readable layout name
- `capsLock: bool` — Caps Lock state
- `numLock: bool` — Num Lock state
- `main: bool` — whether this is the main keyboard

#### Switch layout

- `hyprctl switchxkblayout DEVICE next` — next layout
- `hyprctl switchxkblayout DEVICE prev` — previous layout
- `hyprctl switchxkblayout DEVICE INDEX` — specific layout by index

DEVICE can be: device name, `"current"` (main keyboard), or `"all"` (all keyboards).

#### Per-device layouts

Each physical keyboard can have independent layout config:
```
device:at-translated-set-2-keyboard {
  kb_layout = us,ru,ua
  kb_variant = ,,
  kb_options = grp:win_space_toggle
}
```

#### Event socket

Event: `activelayout>>KEYBOARDNAME,LAYOUTNAME`

Caveat: KEYBOARDNAME can contain commas — parse from the right (LAYOUTNAME is the last segment after the final comma).

Command socket: `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`
Event socket: `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`

### XKB concepts

**Layout:** Named mapping of physical keys to characters (e.g. "us", "de", "ru", "fr"). Defined in `/usr/share/X11/xkb/symbols/`.

**Variant:** Alternative mapping within a layout (e.g. "dvorak", "colemak", "colemak_dh" for "us"). Specified alongside layout: `us(dvorak)`.

**Multiple layouts:** Configured as comma-separated list: `us,de,ru`. Number of variants must match layouts (empty = default): `dvorak,,`.

**Common switch options:**
- `grp:alt_shift_toggle` — Alt+Shift
- `grp:caps_toggle` — Caps Lock
- `grp:win_space_toggle` — Super+Space

### Input method frameworks (optional integration)

#### fcitx5

Service: `org.fcitx.Fcitx5` (session bus)
Interface: `org.fcitx.Fcitx.Controller1`

Methods:
- `CurrentInputMethod() -> String` — active input method name
- `SetCurrentIM(name: String)` — switch input method
- `CurrentInputMethodGroup() -> String` — current IM group
- `ListInputMethods() -> ...` — available input methods

#### ibus

Service: `org.freedesktop.IBus` (session bus)

Key interfaces: `org.freedesktop.IBus.Bus` (daemon control), `org.freedesktop.IBus.InputContext` (per-context input).

Note: Input method integration is optional — most users use XKB layouts directly.

## Topics

- `keyboard.layout` — current layout name, index, available layouts, caps/num lock state

## Methods

- `keyboard.next_layout()` — switch to next layout
- `keyboard.prev_layout()` — switch to previous layout
- `keyboard.set_layout(index: u32)` — switch to specific layout by index (Hyprland only; niri only supports next/prev)

## Types

```rust
/// Current keyboard state, emitted on `keyboard.layout`
struct KeyboardLayout {
    /// Currently active layout name (e.g. "us", "Romanian")
    current: String,
    /// Index of current layout in the available list
    current_index: u32,
    /// All configured layout names
    available: Vec<String>,
    /// Caps Lock state (None if compositor doesn't report it)
    caps_lock: Option<bool>,
    /// Num Lock state (None if compositor doesn't report it)
    num_lock: Option<bool>,
}
```

## Icons

- `input-keyboard-symbolic` — keyboard device
- `preferences-desktop-keyboard-symbolic` — keyboard settings

All icons above are available in Adwaita icon theme.

## Crates

- `niri-ipc` — niri IPC bindings (typed `KeyboardLayouts` struct)
- `hyprland` (0.4) — Hyprland IPC wrapper (async, typed)
- `zbus` (5) — D-Bus client for fcitx5/ibus (optional)

## Change Detection

**Niri:** `EventStream` IPC — send `Request::EventStream`, receive continuous events including layout changes. Fully reactive.

**Hyprland:** `activelayout>>KEYBOARDNAME,LAYOUTNAME` event on the event socket. Fully reactive. Parse carefully — keyboard name may contain commas.

**Caps Lock / Num Lock:**
- Hyprland: available in `hyprctl devices -j` response — poll or re-query after `activelayout` event
- Niri: not exposed via IPC

**Input methods (fcitx5/ibus):** `PropertiesChanged` D-Bus signals on session bus.

## Features

- Report current keyboard layout name and index
- List all configured layouts
- Switch to next/previous layout
- Switch to specific layout by index (Hyprland)
- Caps Lock / Num Lock indicator state (Hyprland)
- Per-keyboard layout tracking (Hyprland, multiple keyboards)
- Input method framework integration (fcitx5, ibus)
- Compositor auto-detection (niri vs hyprland)

## Notes

- Entirely compositor-specific — no universal Wayland protocol for keyboard layout management
- Niri only supports next/prev switching, not jumping to a specific index
- Hyprland's `activelayout` event has a parsing caveat: keyboard name can contain commas
- Caps Lock / Num Lock state is only available from Hyprland; niri doesn't expose it via IPC
- XKB layout files are at `/usr/share/X11/xkb/symbols/` — can be enumerated for a "browse layouts" feature
- Input method frameworks (fcitx5, ibus) operate at a layer above XKB — they intercept input and produce composed characters
