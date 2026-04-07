# MPRIS Layout Artwork Refinement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the MPRIS applet layout so the panel item stays content-sized, add marquee behavior for long panel labels, and upgrade the popover to notification-style player cards with real artwork loading from `art_url`.

**Architecture:** Keep the existing daemon/provider contract unchanged and refine only the panel-side MPRIS applet. The panel label remains a compact single-player surface, while the popover becomes a card list that interprets the single player-provided `art_url` by scheme and falls back cleanly to a symbolic icon.

**Tech Stack:** Rust, GTK4, Relm4, gdk-pixbuf, gio/glib, existing theme.css

---

### Task 1: Fix Panel Layout And Marquee Label

**Files:**
- Modify: `glimpse-panel/src/applets/mpris/applet.rs`
- Test: `glimpse-panel/src/applets/mpris/applet.rs`

- [ ] **Step 1: Write the failing panel helper tests**

```rust
#[test]
fn marquee_text_keeps_original_label_content() {
    let player = CurrentPlayer {
        artist: "Nils Frahm".into(),
        track: "Says".into(),
        identity: "Spotify".into(),
        ..CurrentPlayer::default()
    };

    assert_eq!(panel_label(&player, "{artist} - {track}"), "Nils Frahm - Says");
}

#[test]
fn panel_label_falls_back_to_identity_when_all_metadata_is_missing() {
    let player = CurrentPlayer {
        identity: "Firefox".into(),
        ..CurrentPlayer::default()
    };

    assert_eq!(panel_label(&player, "{artist} - {track}"), "Firefox");
}
```

- [ ] **Step 2: Run the targeted applet tests and verify the baseline still passes**

Run: `cargo test -p glimpse-panel mpris::applet::tests -- --nocapture`

Expected: PASS before the layout change, confirming the fallback behavior remains covered while panel layout work is added.

- [ ] **Step 3: Make the panel widget strictly content-sized and add marquee behavior**

```rust
gtk::Box {
    set_orientation: gtk::Orientation::Horizontal,
    set_spacing: 4,
    add_css_class: "applet",
    add_css_class: "mpris",
    #[watch]
    set_visible: !model.hidden,

    gtk::Label {
        #[watch]
        set_label: &model.label,
        set_xalign: 0.0,
        set_ellipsize: gtk::pango::EllipsizeMode::None,
        set_wrap: false,
        set_single_line_mode: true,
        set_valign: gtk::Align::Center,
        add_css_class: "mpris-label",
        #[watch]
        set_visible: !model.label.is_empty(),
    },
}
```

```rust
fn build_marquee_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label.set_single_line_mode(true);
    label.set_wrap(false);
    label.set_valign(gtk::Align::Center);
    label
}
```

Notes:
- Do not use `set_hexpand(true)` on the panel widget or its children.
- Do not add CSS `min-width` to the panel applet.
- If marquee needs periodic updates, keep it local to the applet and only active when the label text is wider than the available allocation.

- [ ] **Step 4: Run targeted tests and compile check**

Run: `cargo test -p glimpse-panel mpris::applet::tests -- --nocapture`

Expected: PASS

Run: `cargo check -p glimpse-panel`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add glimpse-panel/src/applets/mpris/applet.rs
git commit -m "fix: refine mpris panel label layout"
```

### Task 2: Convert Player Rows Into Cards And Load Artwork

**Files:**
- Modify: `glimpse-panel/src/applets/mpris/popover.rs`
- Modify: `theme.css`
- Test: `glimpse-panel/src/applets/mpris/popover.rs`

- [ ] **Step 1: Write the failing popover helper tests**

```rust
#[test]
fn artwork_source_parses_file_urls() {
    assert_eq!(
        parse_art_source("file:///tmp/cover.jpg"),
        ArtSource::File("/tmp/cover.jpg".into())
    );
}

#[test]
fn artwork_source_parses_https_urls() {
    assert_eq!(
        parse_art_source("https://example.com/cover.jpg"),
        ArtSource::Remote("https://example.com/cover.jpg".into())
    );
}

#[test]
fn artwork_source_falls_back_for_unknown_values() {
    assert_eq!(parse_art_source(""), ArtSource::Fallback);
}

#[test]
fn subtitle_falls_back_to_album_then_identity() {
    let player = PlayerRow {
        album: "Promises".into(),
        identity: "Spotify".into(),
        ..PlayerRow::default()
    };

    assert_eq!(row_subtitle(&player), "Promises");
}
```

- [ ] **Step 2: Run the targeted popover tests and verify they fail**

Run: `cargo test -p glimpse-panel mpris::popover::tests -- --nocapture`

Expected: FAIL with missing `ArtSource` and `parse_art_source`.

- [ ] **Step 3: Add artwork-source parsing and asynchronous image loading**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
enum ArtSource {
    File(String),
    Remote(String),
    Fallback,
}

fn parse_art_source(value: &str) -> ArtSource {
    if let Some(path) = value.strip_prefix("file://") {
        ArtSource::File(path.to_string())
    } else if value.starts_with("http://") || value.starts_with("https://") {
        ArtSource::Remote(value.to_string())
    } else if value.starts_with('/') {
        ArtSource::File(value.to_string())
    } else {
        ArtSource::Fallback
    }
}
```

```rust
fn load_player_art(image: &gtk::Image, art_url: &str) {
    match parse_art_source(art_url) {
        ArtSource::File(path) => {
            let file = gio::File::for_path(path);
            image.set_from_file(Some(&file));
        }
        ArtSource::Remote(url) => {
            let image = image.clone();
            glib::spawn_future_local(async move {
                if let Ok(file) = gio::File::for_uri(&url).read_future(glib::Priority::default()).await {
                    if let Ok(stream) = gdk_pixbuf::Pixbuf::from_stream_at_scale(
                        &file,
                        56,
                        56,
                        true,
                        gio::Cancellable::NONE,
                    ) {
                        image.set_from_pixbuf(Some(&stream));
                    }
                }
            });
        }
        ArtSource::Fallback => image.set_icon_name(Some("audio-x-generic-symbolic")),
    }
}
```

- [ ] **Step 4: Replace bare rows with notification-style player cards**

```rust
let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
card.add_css_class("mpris-card");

let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
let art = gtk::Image::from_icon_name("audio-x-generic-symbolic");
art.set_pixel_size(56);
art.set_valign(gtk::Align::Center);
art.add_css_class("mpris-card-art");
load_player_art(&art, &player.art_url);
header.append(&art);

let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
text.add_css_class("mpris-card-copy");
let title = gtk::Label::new(Some(&player_title(player)));
title.set_halign(gtk::Align::Start);
title.set_xalign(0.0);
title.add_css_class("mpris-card-title");
text.append(&title);
let subtitle = gtk::Label::new(Some(&row_subtitle(player)));
subtitle.set_halign(gtk::Align::Start);
subtitle.set_xalign(0.0);
subtitle.add_css_class("mpris-card-subtitle");
text.append(&subtitle);
header.append(&text);

let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
controls.add_css_class("mpris-card-controls");
```

Notes:
- Keep every player visually equal.
- Follow the notification-card look rather than the earlier bare row.
- Do not use `hexpand` on the panel applet; `hexpand` inside the popover card body is acceptable if needed for card layout.

- [ ] **Step 5: Add matching CSS in `theme.css`**

```css
.mpris-popover .mpris-card {
    padding: 12px;
    margin: 2px 0;
    border-radius: 12px;
    background: color-mix(in srgb, currentColor 6%, transparent);
}

.mpris-popover .mpris-card-art {
    min-width: 56px;
    min-height: 56px;
}

.mpris-popover .mpris-card-title {
    font-weight: 600;
}

.mpris-popover .mpris-card-subtitle {
    opacity: var(--dim-opacity);
}
```

- [ ] **Step 6: Run popover tests and crate checks**

Run: `cargo test -p glimpse-panel mpris:: -- --nocapture`

Expected: PASS

Run: `cargo check -p glimpsed -p glimpse-panel`

Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add glimpse-panel/src/applets/mpris/popover.rs theme.css
git commit -m "feat: refine mpris artwork cards"
```

### Task 3: Manual Runtime Verification And Tracking Update

**Files:**
- Modify: `docs/superpowers/specs/2026-04-07-mpris-applet-design.md`

- [ ] **Step 1: Run the daemon and panel for live verification**

Run:

```bash
RUST_LOG=info cargo run -p glimpsed
cd glimpse-panel && RUST_LOG=info cargo run
```

Verify:

```text
1. The panel label does not force the applet wider than its content.
2. Long labels marquee instead of trimming or stretching the panel applet.
3. Player cards appear in the popover rather than plain rows.
4. Local file artwork loads.
5. HTTP/HTTPS artwork loads.
6. Missing or broken artwork falls back to the symbolic icon.
7. Previous/play-pause/next still target the clicked player card.
```

- [ ] **Step 2: Update the spec only if implementation details diverged**

```markdown
If any finalized class names, artwork-loading rules, or marquee behavior differ from the approved spec,
update `docs/superpowers/specs/2026-04-07-mpris-applet-design.md` to match the code.
```

- [ ] **Step 3: Update issue state**

Run:

```bash
bd update glimpse-zbx --notes "Manual verification completed for panel layout, marquee behavior, and artwork loading."
```

- [ ] **Step 4: Commit any spec synchronization if needed**

```bash
git add docs/superpowers/specs/2026-04-07-mpris-applet-design.md
git commit -m "docs: sync mpris refinement spec"
```
