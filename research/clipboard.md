# Clipboard Provider

**Source:** Wayland data control protocol (wlr-data-control-unstable-v1 or ext-data-control-v1)

**What it does:** Tracks clipboard history, provides the current clipboard contents, and allows copying/clearing.

## System Interface

### Wayland data control protocols

Two protocol variants (functionally equivalent):
- `wlr-data-control-unstable-v1` — wlroots original (Sway, older Hyprland)
- `ext-data-control-v1` — standardized version (Niri, newer compositors)

The protocol allows a privileged client (like a clipboard manager) to:
- Read the current clipboard selection
- Write to the clipboard
- Monitor clipboard changes via `selection` events

Supported by: Sway, Hyprland, Niri, River, LabWC, COSMIC.

### wl-clipboard CLI (reference)

- `wl-copy TEXT` — copy text to clipboard
- `wl-copy --type image/png < file.png` — copy with MIME type
- `wl-paste` — paste clipboard contents to stdout
- `wl-paste --list-types` — list available MIME types
- `wl-paste --type text/plain` — paste specific MIME type
- `wl-copy --primary` / `wl-paste --primary` — primary selection
- `wl-copy --clear` — clear clipboard

## Topics

- `clipboard.current` — current clipboard content (text, or MIME type + size for non-text)
- `clipboard.history` — recent clipboard entries

## Methods

- `clipboard.copy(content: String)` — copy text to clipboard
- `clipboard.copy_bytes(data: Vec<u8>, mime_type: String)` — copy arbitrary data
- `clipboard.select(index: u32)` — copy a history entry back to clipboard
- `clipboard.clear()` — clear clipboard
- `clipboard.clear_history()` — clear history (not current clipboard)
- `clipboard.pin(index: u32)` — pin a history entry (won't be evicted)
- `clipboard.unpin(index: u32)` — unpin

## Types

```rust
/// Type of clipboard content
enum ClipboardContentType {
    Text,
    Image,
    Html,
    /// Other MIME type
    Other(String),
}

/// A clipboard entry
struct ClipboardEntry {
    /// Index in history (0 = most recent)
    index: u32,
    /// Primary MIME type
    mime_type: String,
    content_type: ClipboardContentType,
    /// Text preview (first ~200 chars, or description for non-text)
    preview: String,
    /// Size in bytes
    size: u64,
    /// Timestamp when copied
    timestamp: u64,
    /// Whether this entry is pinned
    pinned: bool,
    /// Application that copied (if detectable)
    source_app: Option<String>,
}

/// Current clipboard state, emitted on `clipboard.current`
struct ClipboardCurrent {
    /// Available MIME types
    mime_types: Vec<String>,
    /// Text content (if text, up to a size limit)
    text: Option<String>,
    /// Content type classification
    content_type: ClipboardContentType,
    /// Size in bytes
    size: u64,
}

/// Clipboard history, emitted on `clipboard.history`
struct ClipboardHistory {
    entries: Vec<ClipboardEntry>,
    /// Maximum history size
    max_size: u32,
}
```

## Icons

- `edit-paste-symbolic` — clipboard/paste
- `edit-copy-symbolic` — copy action
- `edit-clear-symbolic` — clear clipboard
- `user-trash-symbolic` — delete entry

All icons above are available in Adwaita icon theme.

## Crates

- `wl-clipboard-rs` — safe Rust Wayland clipboard access via data control protocols (read, write, monitor)
- `wayland-client` — low-level alternative for direct protocol access

## Change Detection

**Wayland `selection` event:** The data control protocol emits a `selection` event whenever the clipboard contents change. The `wl-clipboard-rs` crate provides a stream of these events. Fully reactive.

## Features

- Monitor clipboard changes in real-time
- Clipboard history with configurable max size
- Text preview for all entries
- Image and rich content support (by MIME type)
- Pin entries to prevent eviction
- Copy text or arbitrary data to clipboard
- Select from history to re-copy
- Clear clipboard and history
- Primary selection support (middle-click paste)
- MIME type listing for current clipboard
- Source application tracking (when available)

## Notes

- Clipboard history is stored in-memory by the daemon — lost on restart (consider optional disk persistence)
- Large clipboard entries (images, files) should store a preview/thumbnail, not the full data
- The Wayland protocol requires the daemon to be a Wayland client — needs `$WAYLAND_DISPLAY`
- Primary selection (middle-click) is a separate selection from the regular clipboard
- Privacy: clipboard history may contain passwords — consider auto-expiry for entries from password managers
- Max history size should be configurable (default ~100 entries)
