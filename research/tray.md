# Tray Provider

**Source:** StatusNotifierItem/Watcher D-Bus (`org.kde.StatusNotifierWatcher`, session bus), DBusMenu (`com.canonical.dbusmenu`)

**What it does:** Discovers system tray items, exposes their icons/tooltips/menus, and handles activation (click) and menu item invocation.

## System Interface

### org.kde.StatusNotifierWatcher (object: `/StatusNotifierWatcher`)

The daemon acts as the tray watcher — applications register their tray items here.

Methods:
- `RegisterStatusNotifierItem(service: String)` — app registers its tray item
- `RegisterStatusNotifierHost(service: String)` — panel registers as tray host

Properties:
- `RegisteredStatusNotifierItems: Vec<String>` (RO) — registered item service names
- `IsStatusNotifierHostRegistered: bool` (RO)
- `ProtocolVersion: i32` (RO)

Signals:
- `StatusNotifierItemRegistered(service: String)`
- `StatusNotifierItemUnregistered(service: String)`
- `StatusNotifierHostRegistered()`

### org.kde.StatusNotifierItem (per-app, object path varies)

Methods:
- `Activate(x: i32, y: i32)` — left-click action
- `SecondaryActivate(x: i32, y: i32)` — middle-click action
- `ContextMenu(x: i32, y: i32)` — right-click, show context menu
- `Scroll(delta: i32, orientation: String)` — scroll; orientation is "vertical" or "horizontal"

Properties:
- `Category: String` (RO) — "ApplicationStatus", "Communications", "SystemServices", "Hardware", "Other"
- `Id: String` (RO) — unique identifier
- `Title: String` (RO) — display title
- `Status: String` (RO) — "Active", "Attention", "Passive"
- `WindowId: i32` (RO) — associated window (0 = none)
- `IconName: String` (RO) — icon name per freedesktop spec
- `IconPixmap: Vec<(i32, i32, Vec<u8>)>` (RO) — array of (width, height, ARGB data)
- `OverlayIconName: String` (RO)
- `OverlayIconPixmap: Vec<(i32, i32, Vec<u8>)>` (RO)
- `AttentionIconName: String` (RO)
- `AttentionIconPixmap: Vec<(i32, i32, Vec<u8>)>` (RO)
- `AttentionMovieName: String` (RO) — animated icon
- `ToolTip: (String, Vec<(i32, i32, Vec<u8>)>, String, String)` (RO) — (icon_name, icon_pixmap, title, body)
- `ItemIsMenu: bool` (RO) — if true, left-click should show menu instead of Activate
- `Menu: ObjectPath` (RO) — D-Bus path to DBusMenu interface

Signals:
- `NewTitle()`
- `NewIcon()`
- `NewAttentionIcon()`
- `NewOverlayIcon()`
- `NewToolTip()`
- `NewStatus(status: String)`

### com.canonical.dbusmenu (object: from StatusNotifierItem.Menu)

Methods:
- `GetLayout(parent_id: i32, recursion_depth: i32, property_names: Vec<String>) -> (u32, (i32, HashMap<String, Variant>, Vec<Variant>))` — get menu tree; parent_id 0 = root, recursion_depth -1 = all
- `Event(id: i32, event_id: String, data: Variant, timestamp: u32)` — trigger menu item action; event_id is typically "clicked"
- `AboutToShow(id: i32) -> bool` — notify submenu about to display

Properties:
- `Version: u32` (RO)
- `TextDirection: String` (RO) — "ltr" or "rtl"
- `Status: String` (RO) — "normal" or "notice"
- `IconThemePath: Vec<String>` (RO)

Signals:
- `ItemsPropertiesUpdated(updates: Vec<(i32, HashMap<String, Variant>)>, removed: Vec<Variant>)` — menu items changed
- `LayoutUpdated(revision: u32, parent: i32)` — menu structure changed

### Host registration

The daemon must:
1. Claim `org.kde.StatusNotifierWatcher` on session bus (become the watcher)
2. Or register as a host via `RegisterStatusNotifierHost()`
3. Monitor `StatusNotifierItemRegistered`/`Unregistered` signals
4. For each item: read properties, subscribe to signals, fetch menu via DBusMenu

## Topics

- `tray.items` — list of all tray items with icons and status
- `tray.item.{id}` — single item state (icon, tooltip, status)
- `tray.item.{id}.menu` — menu tree for an item

## Methods

- `tray.activate(item_id: String, x: i32, y: i32)` — left-click activation
- `tray.secondary_activate(item_id: String, x: i32, y: i32)` — middle-click
- `tray.context_menu(item_id: String, x: i32, y: i32)` — right-click
- `tray.scroll(item_id: String, delta: i32, orientation: String)` — scroll event
- `tray.activate_menu_item(item_id: String, menu_item_id: i32)` — trigger a specific menu item

## Types

```rust
/// Tray item status
enum TrayItemStatus {
    /// Normal operational state
    Active,
    /// Requesting user attention
    Attention,
    /// Not actively interesting (may be hidden)
    Passive,
}

/// Tray item category
enum TrayItemCategory {
    ApplicationStatus,
    Communications,
    SystemServices,
    Hardware,
    Other,
}

/// Icon data — either a theme name or raw pixel data
enum TrayIcon {
    /// Freedesktop icon name
    Name(String),
    /// Raw ARGB pixel data (width, height, data)
    Pixmap { width: i32, height: i32, data: Vec<u8> },
}

/// A tooltip
struct TrayTooltip {
    icon: Option<TrayIcon>,
    title: String,
    body: String,
}

/// A system tray item
struct TrayItem {
    /// Unique identifier (service name or derived)
    id: String,
    category: TrayItemCategory,
    status: TrayItemStatus,
    title: String,
    icon: Option<TrayIcon>,
    overlay_icon: Option<TrayIcon>,
    attention_icon: Option<TrayIcon>,
    tooltip: Option<TrayTooltip>,
    /// Whether left-click should show menu instead of activate
    item_is_menu: bool,
    /// Whether a D-Bus menu is available
    has_menu: bool,
}

/// A menu item in a tray item's context menu
struct TrayMenuItem {
    id: i32,
    label: String,
    enabled: bool,
    visible: bool,
    /// "standard", "separator", "checkmark", "radio"
    item_type: String,
    /// Whether checkmark/radio is toggled
    toggle_state: Option<bool>,
    icon_name: Option<String>,
    children: Vec<TrayMenuItem>,
}
```

## Icons

Tray items provide their own icons via `IconName` or `IconPixmap`. No standard tray-specific icons needed.

General:
- `application-x-executable-symbolic` — generic application fallback

## Crates

- `system-tray` (0.8.5) — async StatusNotifierItem client with DBusMenu support, icon handling, event streams. Already in workspace. Recommended.
- `zbus` (5) — alternative: raw D-Bus for custom watcher implementation

## Change Detection

**Fully reactive via D-Bus signals:**

- `StatusNotifierItemRegistered`/`Unregistered` — items appear/disappear
- Per-item signals: `NewIcon`, `NewTitle`, `NewStatus`, `NewToolTip`, `NewOverlayIcon`, `NewAttentionIcon`
- DBusMenu: `LayoutUpdated` — menu structure changed, `ItemsPropertiesUpdated` — menu item properties changed

No polling needed.

## Features

- Discover and list all system tray items
- Icon display (theme name or raw ARGB pixmap)
- Overlay and attention icon support
- Tooltip display (icon + title + body)
- Left-click activation
- Middle-click secondary activation
- Right-click context menu
- Scroll events
- Menu tree fetching and item activation
- Item status tracking (active/attention/passive)
- Item category classification
- Animated attention icons
- Menu checkbox/radio toggle state
- Icon theme path support for custom app icons

## Notes

- Icon pixmap data is ARGB format — may need conversion to BGRA for display
- `ItemIsMenu` = true means left-click should show menu, not call Activate
- The daemon should either be the watcher (claim `org.kde.StatusNotifierWatcher`) or register as host
- `system-tray` crate handles most of the complexity including DBusMenu diffing
- Some apps use `XAyatanaNewLabel` signal for dynamic text labels (Ubuntu-specific extension)
- Menu items may have submenu children — must handle recursion in `GetLayout`
