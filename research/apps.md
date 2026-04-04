# Apps Provider

**Source:** .desktop files (freedesktop Desktop Entry spec), XDG directories

**What it does:** Indexes installed applications from .desktop files, provides search with ranking, and launches applications.

## System Interface

### .desktop file locations (in priority order)

- `$XDG_DATA_HOME/applications/` (typically `~/.local/share/applications/`)
- `$XDG_DATA_DIRS/applications/` (typically `/usr/local/share/applications/`, `/usr/share/applications/`)
- Flatpak: `~/.local/share/flatpak/exports/share/applications/`, `/var/lib/flatpak/exports/share/applications/`
- Snap: `/var/lib/snapd/desktop/applications/`

### .desktop file format (INI-like)

```ini
[Desktop Entry]
Type=Application
Name=Firefox
GenericName=Web Browser
Comment=Browse the World Wide Web
Exec=firefox %u
Icon=firefox
Terminal=false
Categories=Network;WebBrowser;
Keywords=Internet;WWW;Browser;
MimeType=text/html;application/xhtml+xml;
StartupNotify=true
StartupWMClass=firefox
Actions=new-window;new-private-window;

[Desktop Action new-window]
Name=Open a New Window
Exec=firefox --new-window

[Desktop Action new-private-window]
Name=Open a New Private Window
Exec=firefox --private-window
```

Key fields:
- `Type` — must be "Application" for launchable apps
- `Name` — display name (localized: `Name[de]=Firefox`)
- `GenericName` — generic description (e.g. "Web Browser")
- `Comment` — tooltip text
- `Exec` — command to execute; field codes: `%f` (file), `%u` (URI), `%F` (files), `%U` (URIs)
- `Icon` — icon name (freedesktop icon theme lookup) or absolute path
- `Terminal` — run in terminal emulator
- `NoDisplay` — hidden from menus (true = don't show)
- `Hidden` — deleted by user (true = treat as non-existent)
- `Categories` — semicolon-separated categories (per freedesktop menu spec)
- `Keywords` — semicolon-separated search keywords (localized)
- `MimeType` — semicolon-separated MIME types this app handles
- `StartupWMClass` — WM_CLASS for window matching
- `StartupNotify` — supports startup notification
- `Actions` — semicolon-separated action names (defined in `[Desktop Action ...]` groups)

### Icon resolution

Icons referenced by name (not path) are resolved via freedesktop Icon Theme Specification:
1. Look in current icon theme at `/usr/share/icons/{theme}/`
2. Check sizes: scalable, then closest size match
3. Fall back to `hicolor` theme
4. Fall back to `/usr/share/pixmaps/`

### Launching

On Wayland, launch via the compositor or `gtk-launch`:
- Parse `Exec` field, substitute field codes
- Set `DESKTOP_FILE_ID` environment variable
- Use `g_app_info_launch()` or spawn process directly

## Topics

- `apps.list` — all indexed applications

## Methods

- `apps.search(query: String, max_results: u32) -> Vec<AppEntry>` — fuzzy search by name, generic name, keywords, categories
- `apps.launch(desktop_id: String)` — launch application by .desktop file ID
- `apps.launch_action(desktop_id: String, action: String)` — launch a specific desktop action

## Types

```rust
/// A desktop action (e.g. "Open New Window")
struct DesktopAction {
    /// Action ID from .desktop file
    id: String,
    /// Display name
    name: String,
    /// Exec command
    exec: String,
}

/// An installed application
struct AppEntry {
    /// Desktop file ID (e.g. "firefox.desktop", "org.gnome.Calculator.desktop")
    id: String,
    /// Display name
    name: String,
    /// Generic name (e.g. "Web Browser")
    generic_name: Option<String>,
    /// Description/comment
    comment: Option<String>,
    /// Icon name or path
    icon: Option<String>,
    /// Exec command template
    exec: String,
    /// Whether it runs in a terminal
    terminal: bool,
    /// Semicolon-separated categories
    categories: Vec<String>,
    /// Search keywords
    keywords: Vec<String>,
    /// WM_CLASS for window matching
    startup_wm_class: Option<String>,
    /// Available actions
    actions: Vec<DesktopAction>,
    /// Whether this is a Flatpak app
    is_flatpak: bool,
}

/// Search result with relevance score
struct AppSearchResult {
    app: AppEntry,
    /// Relevance score (higher = better match)
    score: f64,
}
```

## Icons

Apps provide their own icons via the `Icon` field. For the provider itself:
- `system-search-symbolic` — app search/launcher
- `application-x-executable-symbolic` — generic application

## Crates

- `freedesktop-desktop-entry` — .desktop file parsing (efficient, widely used)
- `freedesktop-icons` — icon theme lookup with caching
- `fuzzy-matcher` or `nucleo` — fuzzy string matching for search ranking

## Change Detection

**inotify on .desktop directories:** Watch `$XDG_DATA_HOME/applications/` and each directory in `$XDG_DATA_DIRS/applications/` for file create/modify/delete events. Re-index on change.

**Startup:** Full scan of all .desktop directories to build initial index.

## Features

- Index all installed .desktop applications
- Fuzzy search by name, generic name, keywords, comment, categories
- Frequency-based ranking (track launch count, boost frequently used apps)
- Recently launched tracking
- Launch applications with proper field code substitution
- Launch specific desktop actions (e.g. "New Private Window")
- Category filtering
- Flatpak and Snap app detection
- Localized name/comment support (use current locale)
- Hidden/NoDisplay filtering
- Icon resolution via freedesktop icon theme
- MIME type association lookup
- Window matching via StartupWMClass

## Notes

- Filter out entries with `NoDisplay=true` or `Hidden=true` from search results
- `Exec` field codes (`%u`, `%f`, etc.) must be parsed and substituted or stripped before execution
- Localized keys use `Name[locale]` format — match against current `$LANG`/`$LC_MESSAGES`
- Desktop file ID is typically the filename (e.g. `firefox.desktop`) — for subdirectories, use dash-separated path
- Launch frequency can be stored in a simple JSON file (e.g. `~/.local/share/glimpse/app-usage.json`)
- Re-indexing should be debounced (batch inotify events over ~500ms)
