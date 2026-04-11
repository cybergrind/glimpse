# MPRIS Relm Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the existing MPRIS applet and popover to a real Relm4 component tree with a factory-backed player list while preserving current behavior, controls, and styling.

**Architecture:** Keep `mpris/applet.rs` as the service-facing orchestrator and move the popover internals to declarative Relm4 components. Replace the manual player-row widget map in `mpris/popover.rs` with a factory-backed repeated list and a dedicated `player_row` component that owns card UI, actions, artwork, and progress updates.

**Tech Stack:** Rust, GTK4, Relm4, Relm4 factories, gdk/gio/glib, reqwest

---

### Task 1: Create MPRIS Component Module Structure

**Files:**
- Create: `glimpse-panel/src/applets/mpris/components/mod.rs`
- Create: `glimpse-panel/src/applets/mpris/components/player_row.rs`
- Create: `glimpse-panel/src/applets/mpris/components/player_row_factory.rs`
- Modify: `glimpse-panel/src/applets/mpris/mod.rs`
- Test: `glimpse-panel/src/applets/mpris/components/player_row.rs`

- [ ] **Step 1: Add failing compile-time module references**

```rust
// glimpse-panel/src/applets/mpris/mod.rs
pub mod applet;
pub mod components;
pub mod config;
pub mod popover;
```

```rust
// glimpse-panel/src/applets/mpris/components/mod.rs
pub mod player_row;
pub mod player_row_factory;
```

- [ ] **Step 2: Run a targeted check to verify the new modules are missing**

Run: `cargo check -p glimpse-panel`
Expected: FAIL with missing `components/mod.rs`, `components/player_row.rs`, and `components/player_row_factory.rs`.

- [ ] **Step 3: Add minimal module scaffolding**

```rust
// glimpse-panel/src/applets/mpris/components/player_row.rs
use glimpse::mpris::protocol::MprisPlayer;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk};

pub struct MprisPlayerRow {
    player: MprisPlayer,
}

pub struct MprisPlayerRowInit {
    pub player: MprisPlayer,
    pub show_artwork: bool,
}

#[derive(Debug)]
pub enum MprisPlayerRowInput {
    Update(MprisPlayer),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisPlayerRowOutput {
    Previous { player_id: String },
    PlayPause { player_id: String },
    Next { player_id: String },
    Raise { player_id: String },
}

#[relm4::component(pub)]
impl SimpleComponent for MprisPlayerRow {
    type Init = MprisPlayerRowInit;
    type Input = MprisPlayerRowInput;
    type Output = MprisPlayerRowOutput;

    view! {
        gtk::Box {}
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = MprisPlayerRow { player: init.player };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let MprisPlayerRowInput::Update(player) = msg;
        self.player = player;
    }
}
```

```rust
// glimpse-panel/src/applets/mpris/components/player_row_factory.rs
use glimpse::mpris::protocol::MprisPlayer;

#[derive(Debug, Clone)]
pub struct MprisPlayerRowItem {
    pub player: MprisPlayer,
}
```

- [ ] **Step 4: Add a minimal pure test for the new row output enum file**

```rust
#[cfg(test)]
mod tests {
    use super::MprisPlayerRowOutput;

    #[test]
    fn row_output_variants_are_comparable() {
        assert_eq!(
            MprisPlayerRowOutput::Raise { player_id: "spotify".into() },
            MprisPlayerRowOutput::Raise { player_id: "spotify".into() }
        );
    }
}
```

- [ ] **Step 5: Run the check again**

Run: `cargo check -p glimpse-panel`
Expected: PASS or only existing unrelated warnings.

- [ ] **Step 6: Commit**

```bash
git add glimpse-panel/src/applets/mpris/mod.rs glimpse-panel/src/applets/mpris/components/mod.rs glimpse-panel/src/applets/mpris/components/player_row.rs glimpse-panel/src/applets/mpris/components/player_row_factory.rs
git commit -m "refactor: scaffold mpris relm components"
```

### Task 2: Move One Player Card Into A Real Relm Component

**Files:**
- Modify: `glimpse-panel/src/applets/mpris/components/player_row.rs`
- Modify: `glimpse-panel/src/applets/mpris/popover.rs`
- Test: `glimpse-panel/src/applets/mpris/components/player_row.rs`

- [ ] **Step 1: Write failing pure tests for row mapping helpers**

```rust
#[test]
fn title_prefers_track_then_identity() {
    let mut player = test_player();
    player.title = "Says".into();
    assert_eq!(player_title(&player), "Says");

    player.title.clear();
    player.identity = "Spotify".into();
    assert_eq!(player_title(&player), "Spotify");
}

#[test]
fn shows_progress_only_with_position_length_and_flag() {
    let mut player = test_player();
    player.progress_visible = true;
    player.position = Some(5);
    player.length = Some(10);
    assert!(shows_progress(&player));

    player.length = None;
    assert!(!shows_progress(&player));
}
```

- [ ] **Step 2: Run the targeted tests and verify they fail or are still missing in the new row file**

Run: `cargo test -p glimpse-panel mpris::components::player_row -- --nocapture`
Expected: FAIL with missing helper functions and/or missing test fixture.

- [ ] **Step 3: Move the row-only pure helpers into `player_row.rs`**

```rust
fn player_title(player: &MprisPlayer) -> String {
    if !player.title.is_empty() {
        player.title.clone()
    } else {
        player.identity.clone()
    }
}

fn play_pause_icon(status: MprisPlaybackStatus) -> &'static str {
    match status {
        MprisPlaybackStatus::Playing => "media-playback-pause-symbolic",
        MprisPlaybackStatus::Paused | MprisPlaybackStatus::Stopped => {
            "media-playback-start-symbolic"
        }
    }
}

fn shows_progress(player: &MprisPlayer) -> bool {
    matches!((player.position, player.length), (Some(_), Some(length)) if player.progress_visible && length > 0)
}

fn format_duration(value_micros: u64) -> String {
    let total_seconds = value_micros / 1_000_000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn progress_fraction(position: u64, length: u64) -> f64 {
    if length == 0 {
        0.0
    } else {
        (position as f64 / length as f64).clamp(0.0, 1.0)
    }
}
```

- [ ] **Step 4: Replace the row widget builder with a real `view!` card shell**

```rust
view! {
    gtk::Box {
        set_orientation: gtk::Orientation::Horizontal,
        set_spacing: 12,
        add_css_class: "mpris-card",

        #[name(artwork_box)]
        gtk::Overlay {
            set_visible: model.show_artwork,
            add_css_class: "mpris-card-art",

            #[name(artwork_picture)]
            gtk::Picture {
                set_can_shrink: true,
                set_keep_aspect_ratio: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
            },

            #[name(artwork_fallback)]
            gtk::Image {
                set_icon_name: Some("audio-x-generic-symbolic"),
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                add_css_class: "mpris-card-art-fallback",
            },
        },

        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            set_hexpand: true,
            add_css_class: "mpris-card-content",

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 2,
                    set_hexpand: true,
                    add_css_class: "mpris-card-copy",

                    #[name(title_label)]
                    gtk::Label {
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_wrap: true,
                        add_css_class: "mpris-card-title",
                    },

                    #[name(artist_label)]
                    gtk::Label {
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_wrap: true,
                        add_css_class: "mpris-card-subtitle",
                    },
                },

                #[name(controls_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,
                    add_css_class: "mpris-card-controls",
                },
            },

            #[name(progress_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "mpris-card-progress",

                #[name(progress_start)]
                gtk::Label {},
                #[name(progress_bar)]
                gtk::ProgressBar { set_hexpand: true },
                #[name(progress_end)]
                gtk::Label {},
            },
        },
    }
}
```

- [ ] **Step 5: Keep imperative code only for truly dynamic parts**

```rust
struct MprisPlayerRow {
    player: MprisPlayer,
    show_artwork: bool,
    artwork_revision: Rc<Cell<u64>>,
    current_artwork: MprisArtwork,
    title_label: gtk::Label,
    artist_label: gtk::Label,
    artwork_box: gtk::Overlay,
    artwork_picture: gtk::Picture,
    artwork_fallback: gtk::Image,
    controls_box: gtk::Box,
    progress_box: gtk::Box,
    progress_start: gtk::Label,
    progress_bar: gtk::ProgressBar,
    progress_end: gtk::Label,
}

fn refresh(&mut self, sender: &ComponentSender<Self>) {
    self.title_label.set_label(&player_title(&self.player));
    self.artist_label.set_label(&self.player.subtitle);
    self.artist_label.set_visible(!self.player.subtitle.is_empty());
    self.progress_box.set_visible(shows_progress(&self.player));
    rebuild_controls(&self.controls_box, &self.player, sender);
    refresh_progress_widgets(self);
    refresh_artwork_widgets(self);
}
```

- [ ] **Step 6: Run the targeted row tests and applet check**

Run: `cargo test -p glimpse-panel mpris::components::player_row -- --nocapture && cargo check -p glimpse-panel`
Expected: PASS, with only existing unrelated warnings allowed.

- [ ] **Step 7: Commit**

```bash
git add glimpse-panel/src/applets/mpris/components/player_row.rs glimpse-panel/src/applets/mpris/popover.rs
git commit -m "refactor: convert mpris player row to relm component"
```

### Task 3: Convert The Popover Shell To Declarative Relm And Add A Factory

**Files:**
- Modify: `glimpse-panel/src/applets/mpris/popover.rs`
- Modify: `glimpse-panel/src/applets/mpris/components/player_row_factory.rs`
- Test: `glimpse-panel/src/applets/mpris/popover.rs`

- [ ] **Step 1: Write failing pure tests for visible-row truncation**

```rust
#[test]
fn visible_players_respects_max_rows() {
    let players = vec![
        test_player_with_id("one"),
        test_player_with_id("two"),
        test_player_with_id("three"),
    ];

    let ids: Vec<String> = visible_players(players, 2)
        .into_iter()
        .map(|player| player.player_id)
        .collect();

    assert_eq!(ids, vec!["one", "two"]);
}
```

- [ ] **Step 2: Run the focused tests and verify the popover still depends on the manual row map**

Run: `cargo test -p glimpse-panel mpris::popover -- --nocapture`
Expected: PASS for old pure tests but code still contains `HashMap<String, PlayerRowWidgets>` and `build_row()` / `update_row()` helpers. This is the baseline before replacement.

- [ ] **Step 3: Introduce the factory item type and identity contract**

```rust
// glimpse-panel/src/applets/mpris/components/player_row_factory.rs
use glimpse::mpris::protocol::MprisPlayer;
use relm4::factory::{DynamicIndex, FactoryComponent};

use super::player_row::{MprisPlayerRow, MprisPlayerRowInit, MprisPlayerRowInput, MprisPlayerRowOutput};

#[derive(Debug, Clone)]
pub struct MprisPlayerRowItem {
    pub player: MprisPlayer,
    pub show_artwork: bool,
}

impl MprisPlayerRowItem {
    pub fn key(&self) -> &str {
        &self.player.player_id
    }
}
```

- [ ] **Step 4: Replace the manual `HashMap` row map with a factory-backed list**

```rust
use relm4::factory::{FactoryVecDeque, DynamicIndex};

pub struct MprisPopover {
    popover: gtk::Popover,
    empty_label: gtk::Label,
    max_rows: usize,
    show_artwork: bool,
    rows: FactoryVecDeque<MprisPlayerRowItem>,
}

fn sync_rows(&mut self, players: Vec<MprisPlayer>) {
    let players = visible_players(players, self.max_rows);
    self.empty_label.set_visible(players.is_empty());

    let mut guard = self.rows.guard();
    guard.clear();
    for player in players {
        guard.push_back(MprisPlayerRowItem {
            player,
            show_artwork: self.show_artwork,
        });
    }
}
```

- [ ] **Step 5: Make the popover shell declarative and mount the factory in `view!`**

```rust
view! {
    root = gtk::Popover {
        add_css_class: "mpris-popover",

        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,

            #[name(empty_label)]
            gtk::Label {
                set_label: "No media players",
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
            },

            gtk::ScrolledWindow {
                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                set_propagate_natural_height: true,

                #[local_ref]
                rows_box -> gtk::Box {},
            },
        },
    }
}
```

```rust
let rows = FactoryVecDeque::builder()
    .launch_default()
    .forward(sender.output_sender(), |output| output);
let rows_box = rows.widget().clone();
```

- [ ] **Step 6: Remove obsolete manual row helpers**

Delete from `mpris/popover.rs` once the new factory path is live:

```rust
struct PlayerRowWidgets { ... }
fn build_row(...) -> PlayerRowWidgets { ... }
fn update_row(...) { ... }
```

- [ ] **Step 7: Run focused tests and the full applet check**

Run: `cargo test -p glimpse-panel mpris::popover -- --nocapture && cargo check -p glimpse-panel`
Expected: PASS, with the manual row map removed from the file.

- [ ] **Step 8: Commit**

```bash
git add glimpse-panel/src/applets/mpris/popover.rs glimpse-panel/src/applets/mpris/components/player_row_factory.rs
git commit -m "refactor: use factory rows in mpris popover"
```

### Task 4: Wire Row Outputs Through The Factory And Applet Boundary

**Files:**
- Modify: `glimpse-panel/src/applets/mpris/components/player_row.rs`
- Modify: `glimpse-panel/src/applets/mpris/components/player_row_factory.rs`
- Modify: `glimpse-panel/src/applets/mpris/popover.rs`
- Modify: `glimpse-panel/src/applets/mpris/applet.rs`
- Test: `glimpse-panel/src/applets/mpris/applet.rs`

- [ ] **Step 1: Write failing tests for panel command mapping**

```rust
#[test]
fn maps_popover_previous_output_to_previous_command() {
    let output = MprisPopoverOutput::Previous {
        player_id: "spotify".into(),
    };

    let command = command_for_popover_output(output);
    assert_eq!(command, MprisServiceCommand::Previous {
        player_id: "spotify".into(),
    });
}

#[test]
fn maps_popover_raise_output_to_raise_command() {
    let output = MprisPopoverOutput::Raise {
        player_id: "spotify".into(),
    };

    let command = command_for_popover_output(output);
    assert_eq!(command, MprisServiceCommand::Raise {
        player_id: "spotify".into(),
    });
}
```

- [ ] **Step 2: Run the applet tests and verify the helper is missing**

Run: `cargo test -p glimpse-panel mpris::applet -- --nocapture`
Expected: FAIL with missing `command_for_popover_output`.

- [ ] **Step 3: Add an explicit applet-side mapping helper**

```rust
fn command_for_popover_output(output: MprisPopoverOutput) -> MprisServiceCommand {
    match output {
        MprisPopoverOutput::Previous { player_id } => {
            MprisServiceCommand::Previous { player_id }
        }
        MprisPopoverOutput::PlayPause { player_id } => {
            MprisServiceCommand::PlayPause { player_id }
        }
        MprisPopoverOutput::Next { player_id } => MprisServiceCommand::Next { player_id },
        MprisPopoverOutput::Raise { player_id } => MprisServiceCommand::Raise { player_id },
    }
}
```

- [ ] **Step 4: Make the factory and popover forward row output unchanged**

```rust
// player_row_factory.rs
impl FactoryComponent for MprisPlayerRowItem {
    type Init = Self;
    type Input = MprisPlayer;
    type Output = MprisPlayerRowOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = relm4::Controller<MprisPlayerRow>;

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: relm4::FactorySender<Self>) -> Self {
        init
    }
}
```

```rust
// popover.rs
rows.forward(sender.output_sender(), |row_output| match row_output {
    MprisPlayerRowOutput::Previous { player_id } => MprisPopoverOutput::Previous { player_id },
    MprisPlayerRowOutput::PlayPause { player_id } => MprisPopoverOutput::PlayPause { player_id },
    MprisPlayerRowOutput::Next { player_id } => MprisPopoverOutput::Next { player_id },
    MprisPlayerRowOutput::Raise { player_id } => MprisPopoverOutput::Raise { player_id },
});
```

- [ ] **Step 5: Replace the match in `handle_popover_output` to use the helper**

```rust
fn handle_popover_output(&self, output: MprisPopoverOutput, sender: ComponentSender<Self>) {
    self.send_command(sender, command_for_popover_output(output));
}
```

- [ ] **Step 6: Run the applet tests and applet check**

Run: `cargo test -p glimpse-panel mpris::applet -- --nocapture && cargo check -p glimpse-panel`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add glimpse-panel/src/applets/mpris/components/player_row.rs glimpse-panel/src/applets/mpris/components/player_row_factory.rs glimpse-panel/src/applets/mpris/popover.rs glimpse-panel/src/applets/mpris/applet.rs
git commit -m "refactor: wire mpris row outputs through relm factory"
```

### Task 5: Final Cleanup, Verification, And Dead Code Removal

**Files:**
- Modify: `glimpse-panel/src/applets/mpris/popover.rs`
- Modify: `glimpse-panel/src/applets/mpris/components/player_row.rs`
- Modify: `glimpse-panel/src/applets/mpris/applet.rs`
- Test: `glimpse-panel/src/applets/mpris/popover.rs`
- Test: `glimpse-panel/src/applets/mpris/components/player_row.rs`

- [ ] **Step 1: Remove leftover manual-path dead code**

Delete any remaining obsolete items once the factory path is stable:

```rust
use std::collections::HashMap;
struct PlayerRowWidgets { ... }
fn media_button(...) -> gtk::Button { ... } // only if fully replaced by row component helpers
fn build_row(...) -> PlayerRowWidgets { ... }
fn update_row(...) { ... }
```

- [ ] **Step 2: Keep the popover parent-attachment exception explicit and minimal**

```rust
fn init(
    init: Self::Init,
    root: Self::Root,
    _sender: ComponentSender<Self>,
) -> ComponentParts<Self> {
    root.set_parent(&init.parent);
    root.set_autohide(true);
    // all remaining UI composition happens through view! and factory mounting
}
```

- [ ] **Step 3: Run the focused MPRIS tests**

Run: `cargo test -p glimpse-panel mpris:: -- --nocapture`
Expected: PASS for panel-label, artwork-source, reload, progress, and new row/factory tests.

- [ ] **Step 4: Run the full package check**

Run: `cargo check -p glimpse-panel`
Expected: PASS with only existing unrelated warnings.

- [ ] **Step 5: Manual runtime verification**

Run: `cargo run -p glimpse-panel`
Expected:
- the MPRIS panel label still appears and hides correctly
- the popover still opens and closes on click
- player cards still render title, subtitle, controls, and progress
- artwork still loads from file and remote URLs
- previous/play-pause/next/open still work
- empty state still appears when there are no visible players

- [ ] **Step 6: Commit**

```bash
git add glimpse-panel/src/applets/mpris/applet.rs glimpse-panel/src/applets/mpris/popover.rs glimpse-panel/src/applets/mpris/components/player_row.rs glimpse-panel/src/applets/mpris/components/player_row_factory.rs
git commit -m "refactor: complete mpris relm popover migration"
```
