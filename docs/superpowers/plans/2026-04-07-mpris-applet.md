# MPRIS Applet Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a daemon-backed MPRIS provider and panel applet that show the most recently active player in the panel and a flat multi-player popover with previous, play/pause, and next controls.

**Architecture:** The `glimpsed` daemon owns MPRIS D-Bus discovery, player state normalization, recency selection, and media control methods. The `glimpse-panel` applet subscribes to `mpris.current` and `mpris.players`, renders one compact panel label, and shows a sorted list of equal player rows in a popover.

**Tech Stack:** Rust, Tokio, zbus, serde/serde_json, GTK4, Relm4

---

### Task 1: Add Provider Metadata And Tests

**Files:**
- Create: `glimpsed/src/providers/mpris.rs`
- Modify: `glimpsed/src/providers/mod.rs`
- Modify: `glimpsed/src/main.rs`
- Test: `glimpsed/src/providers/mpris.rs`

- [ ] **Step 1: Write the failing provider metadata tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_metadata() {
        assert_eq!(NAME, "mpris");
        assert_eq!(TOPICS, &["mpris.current", "mpris.players"]);
        assert_eq!(
            METHODS,
            &["mpris.play_pause", "mpris.previous", "mpris.next", "mpris.raise"]
        );
    }

    #[test]
    fn selects_newest_player_for_current() {
        let players = vec![
            PlayerSnapshot {
                player_id: "spotify".into(),
                last_active: 10,
                ..PlayerSnapshot::test_default()
            },
            PlayerSnapshot {
                player_id: "firefox".into(),
                last_active: 20,
                ..PlayerSnapshot::test_default()
            },
        ];

        let current = select_current_player(&players).expect("current player");
        assert_eq!(current.player_id, "firefox");
    }
}
```

- [ ] **Step 2: Run the provider tests and verify they fail**

Run: `cargo test -p glimpsed mpris::tests -- --nocapture`

Expected: FAIL with unresolved module, missing constants, and missing helper types/functions in `glimpsed/src/providers/mpris.rs`.

- [ ] **Step 3: Add initial provider scaffolding**

```rust
use std::pin::Pin;

use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "mpris";
const TOPICS: &[&str] = &["mpris.current", "mpris.players"];
const METHODS: &[&str] = &["mpris.play_pause", "mpris.previous", "mpris.next", "mpris.raise"];

#[derive(Debug, Clone, Serialize, Default)]
struct PlayerSnapshot {
    player_id: String,
    bus_name: String,
    identity: String,
    artist: String,
    track: String,
    album: String,
    status: String,
    art_url: String,
    can_go_previous: bool,
    can_play_pause: bool,
    can_go_next: bool,
    last_active: u64,
}

impl PlayerSnapshot {
    #[cfg(test)]
    fn test_default() -> Self {
        Self {
            player_id: String::new(),
            bus_name: String::new(),
            identity: String::new(),
            artist: String::new(),
            track: String::new(),
            album: String::new(),
            status: "Stopped".into(),
            art_url: String::new(),
            can_go_previous: false,
            can_play_pause: true,
            can_go_next: false,
            last_active: 0,
        }
    }
}

fn select_current_player(players: &[PlayerSnapshot]) -> Option<PlayerSnapshot> {
    players.iter().max_by_key(|player| player.last_active).cloned()
}

pub struct MprisProvider;

impl Provider for MprisProvider {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }

    fn run(
        &mut self,
        _events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            while let Some(req) = tokio::select! {
                _ = cancel.cancelled() => None,
                req = requests.recv() => req,
            } {
                if let ProviderRequest::Snapshot { reply, .. } = req {
                    let _ = reply.send(None);
                }
            }
            Ok(())
        })
    }
}
```

- [ ] **Step 4: Register the provider module**

```rust
// glimpsed/src/providers/mod.rs
pub mod mpris;
```

```rust
// glimpsed/src/main.rs
Box::new(providers::mpris::MprisProviderFactory),
```

```rust
pub struct MprisProviderFactory;

impl ProviderFactory for MprisProviderFactory {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }
    fn create(&self) -> Box<dyn Provider> { Box::new(MprisProvider) }
}
```

- [ ] **Step 5: Run the provider tests and verify they pass**

Run: `cargo test -p glimpsed mpris::tests -- --nocapture`

Expected: PASS with `provider_metadata` and `selects_newest_player_for_current`.

- [ ] **Step 6: Commit**

```bash
git add glimpsed/src/providers/mpris.rs glimpsed/src/providers/mod.rs glimpsed/src/main.rs
git commit -m "feat: scaffold mpris provider"
```

### Task 2: Implement Player Normalization, Recency, And Snapshots

**Files:**
- Modify: `glimpsed/src/providers/mpris.rs`
- Test: `glimpsed/src/providers/mpris.rs`

- [ ] **Step 1: Write failing normalization and fallback tests**

```rust
#[test]
fn label_falls_back_from_artist_and_track_to_identity() {
    let player = PlayerSnapshot {
        identity: "Firefox".into(),
        artist: String::new(),
        track: String::new(),
        ..PlayerSnapshot::test_default()
    };

    assert_eq!(player.panel_label("{artist} - {track}"), "Firefox");
}

#[test]
fn removes_missing_player_and_recomputes_current() {
    let mut state = ProviderState::default();
    state.upsert(PlayerSnapshot {
        player_id: "spotify".into(),
        last_active: 10,
        ..PlayerSnapshot::test_default()
    });
    state.upsert(PlayerSnapshot {
        player_id: "firefox".into(),
        last_active: 20,
        ..PlayerSnapshot::test_default()
    });

    state.remove("firefox");

    assert_eq!(state.players.len(), 1);
    assert_eq!(state.current().unwrap().player_id, "spotify");
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cargo test -p glimpsed mpris::tests -- --nocapture`

Expected: FAIL with missing `ProviderState`, `upsert`, `remove`, and `panel_label`.

- [ ] **Step 3: Add normalized provider state helpers**

```rust
#[derive(Debug, Default)]
struct ProviderState {
    players: Vec<PlayerSnapshot>,
}

impl ProviderState {
    fn upsert(&mut self, player: PlayerSnapshot) {
        if let Some(existing) = self.players.iter_mut().find(|p| p.player_id == player.player_id) {
            *existing = player;
        } else {
            self.players.push(player);
        }
        self.players.sort_by(|a, b| b.last_active.cmp(&a.last_active));
    }

    fn remove(&mut self, player_id: &str) {
        self.players.retain(|player| player.player_id != player_id);
    }

    fn current(&self) -> Option<PlayerSnapshot> {
        self.players.first().cloned()
    }
}

impl PlayerSnapshot {
    fn subtitle(&self) -> String {
        if !self.artist.is_empty() {
            self.artist.clone()
        } else if !self.album.is_empty() {
            self.album.clone()
        } else {
            self.identity.clone()
        }
    }

    fn panel_label(&self, format: &str) -> String {
        let rendered = format
            .replace("{artist}", &self.artist)
            .replace("{track}", &self.track)
            .trim_matches([' ', '-', '—'])
            .trim()
            .to_string();

        if !rendered.is_empty() {
            rendered
        } else if !self.track.is_empty() {
            self.track.clone()
        } else {
            self.identity.clone()
        }
    }
}
```

- [ ] **Step 4: Add snapshot reply handling in the provider**

```rust
match req {
    ProviderRequest::Snapshot { topic, reply } => {
        let data = match topic.as_str() {
            "mpris.current" => serde_json::to_value(self.state.current()).ok(),
            "mpris.players" => serde_json::to_value(&self.state.players).ok(),
            _ => None,
        };
        let _ = reply.send(data);
    }
    ProviderRequest::Call { .. } => { /* keep stub for now */ }
}
```

- [ ] **Step 5: Run the provider tests and verify they pass**

Run: `cargo test -p glimpsed mpris::tests -- --nocapture`

Expected: PASS with fallback, recompute, and metadata tests green.

- [ ] **Step 6: Commit**

```bash
git add glimpsed/src/providers/mpris.rs
git commit -m "feat: add mpris provider state helpers"
```

### Task 3: Implement D-Bus Discovery, Refresh, And Control Methods

**Files:**
- Modify: `glimpsed/src/providers/mpris.rs`
- Test: `glimpsed/src/providers/mpris.rs`

- [ ] **Step 1: Write failing tests for command routing and recency bumping**

```rust
#[test]
fn control_method_updates_last_active_for_target_player() {
    let mut state = ProviderState::default();
    state.upsert(PlayerSnapshot {
        player_id: "spotify".into(),
        last_active: 10,
        ..PlayerSnapshot::test_default()
    });

    state.mark_active("spotify", 99);

    assert_eq!(state.current().unwrap().player_id, "spotify");
    assert_eq!(state.current().unwrap().last_active, 99);
}

#[test]
fn control_method_requires_player_id() {
    let err = parse_player_id(&serde_json::json!({})).unwrap_err().to_string();
    assert!(err.contains("missing 'player_id'"));
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cargo test -p glimpsed mpris::tests -- --nocapture`

Expected: FAIL with missing `mark_active` and `parse_player_id`.

- [ ] **Step 3: Add control parameter parsing and recency helper**

```rust
fn parse_player_id(params: &serde_json::Value) -> anyhow::Result<&str> {
    params["player_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'player_id'"))
}

impl ProviderState {
    fn mark_active(&mut self, player_id: &str, ts: u64) {
        if let Some(player) = self.players.iter_mut().find(|p| p.player_id == player_id) {
            player.last_active = ts;
        }
        self.players.sort_by(|a, b| b.last_active.cmp(&a.last_active));
    }
}
```

- [ ] **Step 4: Implement D-Bus refresh and call dispatch**

```rust
async fn refresh_players(&mut self, events: &mpsc::Sender<ProviderEvent>) -> anyhow::Result<()> {
    let names = list_mpris_names(&self.connection).await?;
    self.state.players.retain(|player| names.contains(&player.bus_name));

    for bus_name in names {
        let snapshot = load_player_snapshot(&self.connection, &bus_name).await?;
        self.state.upsert(snapshot);
    }

    emit_snapshot(events, "mpris.players", &self.state.players).await;
    emit_snapshot(events, "mpris.current", &self.state.current()).await;
    Ok(())
}

match method.as_str() {
    "mpris.play_pause" => call_player_method(&self.connection, player_id, "PlayPause").await,
    "mpris.previous" => call_player_method(&self.connection, player_id, "Previous").await,
    "mpris.next" => call_player_method(&self.connection, player_id, "Next").await,
    "mpris.raise" => call_root_method(&self.connection, player_id, "Raise").await,
    _ => Err(anyhow::anyhow!("unknown method: {method}")),
}
```

- [ ] **Step 5: Wire provider `run()` to D-Bus name-owner changes**

```rust
fn run(
    &mut self,
    events: mpsc::Sender<ProviderEvent>,
    mut requests: mpsc::Receiver<ProviderRequest>,
    cancel: CancellationToken,
) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
    Box::pin(async move {
        self.refresh_players(&events).await?;
        let mut changes = subscribe_name_owner_changes(&self.connection).await?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                req = requests.recv() => {
                    let Some(req) = req else { break };
                    self.handle_request(req, &events).await;
                }
                Some(_) = changes.next() => {
                    self.refresh_players(&events).await?;
                }
            }
        }

        Ok(())
    })
}
```

- [ ] **Step 6: Run the provider tests**

Run: `cargo test -p glimpsed mpris::tests -- --nocapture`

Expected: PASS for command parsing and recency helpers. If D-Bus integration is split behind helpers, unit tests should not require a session bus.

- [ ] **Step 7: Run a compile check for the daemon**

Run: `cargo check -p glimpsed`

Expected: PASS with `glimpsed` compiling cleanly and `mpris` registered.

- [ ] **Step 8: Commit**

```bash
git add glimpsed/src/providers/mpris.rs
git commit -m "feat: implement mpris provider dbus integration"
```

### Task 4: Add Applet Config, Registration, And Panel Label

**Files:**
- Create: `glimpse-panel/src/applets/mpris/mod.rs`
- Create: `glimpse-panel/src/applets/mpris/config.rs`
- Create: `glimpse-panel/src/applets/mpris/applet.rs`
- Modify: `glimpse-panel/src/applets/mod.rs`
- Test: `glimpse-panel/src/applets/mpris/applet.rs`

- [ ] **Step 1: Write failing applet formatting tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_artist_and_track() {
        let player = CurrentPlayer {
            artist: "Nils Frahm".into(),
            track: "Says".into(),
            identity: "Spotify".into(),
            ..CurrentPlayer::default()
        };

        assert_eq!(panel_label(&player, "{artist} - {track}"), "Nils Frahm - Says");
    }

    #[test]
    fn falls_back_to_identity_when_metadata_is_missing() {
        let player = CurrentPlayer {
            identity: "Firefox".into(),
            ..CurrentPlayer::default()
        };

        assert_eq!(panel_label(&player, "{artist} - {track}"), "Firefox");
    }
}
```

- [ ] **Step 2: Run the applet tests and verify they fail**

Run: `cargo test -p glimpse-panel mpris::applet::tests -- --nocapture`

Expected: FAIL with missing module, config, and label helpers.

- [ ] **Step 3: Add applet config and module exports**

```rust
// glimpse-panel/src/applets/mpris/config.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MprisConfig {
    pub label_format: String,
    pub show_artwork: bool,
    pub hide_when_empty: bool,
    pub max_rows: usize,
}

impl Default for MprisConfig {
    fn default() -> Self {
        Self {
            label_format: "{artist} - {track}".into(),
            show_artwork: true,
            hide_when_empty: true,
            max_rows: 6,
        }
    }
}
```

```rust
// glimpse-panel/src/applets/mpris/mod.rs
mod applet;
mod config;
mod popover;

pub use applet::{Mpris, MprisInit};
pub use config::MprisConfig;
```

- [ ] **Step 4: Add the panel applet skeleton and label helper**

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CurrentPlayer {
    pub player_id: String,
    pub identity: String,
    pub artist: String,
    pub track: String,
    pub album: String,
    pub status: String,
}

fn panel_label(player: &CurrentPlayer, format: &str) -> String {
    let label = format
        .replace("{artist}", &player.artist)
        .replace("{track}", &player.track)
        .trim_matches([' ', '-', '—'])
        .trim()
        .to_string();

    if !label.is_empty() {
        label
    } else if !player.track.is_empty() {
        player.track.clone()
    } else {
        player.identity.clone()
    }
}
```

- [ ] **Step 5: Register the applet in the applet factory**

```rust
mod mpris;
```

```rust
pub enum AppletController {
    // ...
    Mpris(Controller<mpris::Mpris>),
}
```

```rust
"mpris" => {
    let client = client.clone()?;
    let config: mpris::MprisConfig = applet_config
        .map(|c| c.settings.clone().try_into().unwrap_or_default())
        .unwrap_or_default();
    let applet = mpris::Mpris::builder()
        .launch(mpris::MprisInit { config, client })
        .detach();
    Some(AppletController::Mpris(applet))
}
```

- [ ] **Step 6: Subscribe to `mpris.current` and update the panel widget**

```rust
sender.command(move |out, shutdown| {
    shutdown
        .register(async move {
            let mut current_sub = match client.subscribe("mpris.current").await {
                Ok(sub) => sub,
                Err(_) => {
                    let _ = out.send(MprisMsg::Unavailable);
                    return;
                }
            };

            loop {
                tokio::select! {
                    Some(ev) = current_sub.next() => {
                        if ev.data.is_null() {
                            let _ = out.send(MprisMsg::ClearCurrent);
                        } else if let Ok(player) = serde_json::from_value(ev.data) {
                            let _ = out.send(MprisMsg::CurrentUpdate(player));
                        }
                    }
                    else => break,
                }
            }
        })
        .drop_on_shutdown()
});
```

- [ ] **Step 7: Run the applet tests and a compile check**

Run: `cargo test -p glimpse-panel mpris::applet::tests -- --nocapture`

Expected: PASS for label formatting tests.

Run: `cargo check -p glimpse-panel`

Expected: PASS with `mpris` applet registration compiling cleanly.

- [ ] **Step 8: Commit**

```bash
git add glimpse-panel/src/applets/mpris/mod.rs glimpse-panel/src/applets/mpris/config.rs glimpse-panel/src/applets/mpris/applet.rs glimpse-panel/src/applets/mod.rs
git commit -m "feat: add mpris panel applet"
```

### Task 5: Build The Flat Multi-Player Popover And Row Actions

**Files:**
- Create: `glimpse-panel/src/applets/mpris/popover.rs`
- Modify: `glimpse-panel/src/applets/mpris/applet.rs`
- Test: `glimpse-panel/src/applets/mpris/popover.rs`

- [ ] **Step 1: Write failing popover row tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_falls_back_to_album_then_identity() {
        let player = PlayerRow {
            album: "Promises".into(),
            identity: "Spotify".into(),
            ..PlayerRow::default()
        };

        assert_eq!(row_subtitle(&player), "Promises");
    }

    #[test]
    fn sort_players_newest_first() {
        let players = vec![
            PlayerRow { player_id: "a".into(), last_active: 1, ..PlayerRow::default() },
            PlayerRow { player_id: "b".into(), last_active: 9, ..PlayerRow::default() },
        ];

        let sorted = sorted_players(players, 6);
        assert_eq!(sorted[0].player_id, "b");
    }
}
```

- [ ] **Step 2: Run the popover tests and verify they fail**

Run: `cargo test -p glimpse-panel mpris::popover::tests -- --nocapture`

Expected: FAIL with missing row helpers and popover model.

- [ ] **Step 3: Add player row data and sorting helpers**

```rust
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PlayerRow {
    pub player_id: String,
    pub identity: String,
    pub artist: String,
    pub track: String,
    pub album: String,
    pub status: String,
    pub art_url: String,
    pub can_go_previous: bool,
    pub can_play_pause: bool,
    pub can_go_next: bool,
    pub last_active: u64,
}

fn row_subtitle(player: &PlayerRow) -> String {
    if !player.artist.is_empty() {
        player.artist.clone()
    } else if !player.album.is_empty() {
        player.album.clone()
    } else {
        player.identity.clone()
    }
}

fn sorted_players(mut players: Vec<PlayerRow>, max_rows: usize) -> Vec<PlayerRow> {
    players.sort_by(|a, b| b.last_active.cmp(&a.last_active));
    players.truncate(max_rows);
    players
}
```

- [ ] **Step 4: Build the popover row layout and control callbacks**

```rust
let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
row.add_css_class("mpris-row");
row.set_valign(gtk::Align::Center);

let art = gtk::Image::from_icon_name("audio-x-generic-symbolic");
art.set_pixel_size(32);
art.add_css_class("mpris-art");
art.set_valign(gtk::Align::Center);

let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
let title = gtk::Label::new(Some(&player.track));
title.add_css_class("mpris-track");
title.set_halign(gtk::Align::Start);
let subtitle = gtk::Label::new(Some(&row_subtitle(player)));
subtitle.add_css_class("mpris-artist");
subtitle.set_halign(gtk::Align::Start);

let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
controls.add_css_class("mpris-controls");
controls.append(&media_button("media-skip-backward-symbolic", player.can_go_previous, move || {
    spawn_call(&client, "mpris.previous", serde_json::json!({ "player_id": player_id }));
}));
controls.append(&media_button(play_pause_icon(&player.status), player.can_play_pause, move || {
    spawn_call(&client, "mpris.play_pause", serde_json::json!({ "player_id": player_id }));
}));
controls.append(&media_button("media-skip-forward-symbolic", player.can_go_next, move || {
    spawn_call(&client, "mpris.next", serde_json::json!({ "player_id": player_id }));
}));
```

- [ ] **Step 5: Wire `mpris.players` updates from applet to popover**

```rust
Some(ev) = async {
    match &mut players_sub {
        Some(sub) => sub.next().await,
        None => std::future::pending().await,
    }
} => {
    if let Ok(players) = serde_json::from_value(ev.data) {
        let _ = out.send(MprisMsg::PlayersUpdate(players));
    }
}
```

```rust
MprisMsg::PlayersUpdate(players) => {
    self.popover.emit(PopoverInput::UpdatePlayers(players));
}
```

- [ ] **Step 6: Run the popover tests and compile check**

Run: `cargo test -p glimpse-panel mpris::popover::tests -- --nocapture`

Expected: PASS for subtitle and sorting helpers.

Run: `cargo check -p glimpse-panel`

Expected: PASS with popover row actions and subscriptions compiling.

- [ ] **Step 7: Commit**

```bash
git add glimpse-panel/src/applets/mpris/applet.rs glimpse-panel/src/applets/mpris/popover.rs
git commit -m "feat: add mpris popover rows"
```

### Task 6: Add Styling And Verify End-To-End Behavior

**Files:**
- Modify: `theme.css`
- Modify: `docs/superpowers/specs/2026-04-07-mpris-applet-design.md`
- Modify: `docs/superpowers/plans/2026-04-07-mpris-applet.md`

- [ ] **Step 1: Add MPRIS-specific CSS rules**

```css
.mpris-popover contents > box {
  margin: var(--popover-padding);
}

.mpris-row {
  padding: 6px 0;
}

.mpris-art {
  min-width: 32px;
  min-height: 32px;
}

.mpris-track {
  font-weight: 600;
}

.mpris-artist {
  opacity: var(--dim-opacity);
}

.mpris-controls button {
  min-width: 28px;
  min-height: 28px;
}
```

- [ ] **Step 2: Run formatting and targeted checks**

Run: `cargo fmt --all`

Expected: PASS with Rust sources formatted.

Run: `cargo check -p glimpsed -p glimpse-panel`

Expected: PASS for both daemon and panel crates.

- [ ] **Step 3: Run targeted tests**

Run: `cargo test -p glimpsed mpris::tests -- --nocapture`

Expected: PASS for provider tests.

Run: `cargo test -p glimpse-panel mpris:: -- --nocapture`

Expected: PASS for applet and popover tests containing `mpris`.

- [ ] **Step 4: Perform manual verification**

Run:

```bash
RUST_LOG=info cargo run -p glimpsed
cd glimpse-panel && RUST_LOG=info cargo run
```

Verify:

```text
1. Start one player and confirm the panel shows one compact label.
2. Start a second player and confirm the panel switches to the most recent one.
3. Open the popover and confirm both players appear as equal rows.
4. Confirm previous/play-pause/next target the clicked row only.
5. Confirm missing artwork falls back to a symbolic icon.
6. Close a player and confirm the row disappears without breaking the popover.
```

- [ ] **Step 5: Update docs if implementation diverges**

```markdown
If any finalized field name, CSS class, or config default differs from the spec or plan,
update `docs/superpowers/specs/2026-04-07-mpris-applet-design.md`
and `docs/superpowers/plans/2026-04-07-mpris-applet.md` in the same change.
```

- [ ] **Step 6: Commit**

```bash
git add theme.css docs/superpowers/specs/2026-04-07-mpris-applet-design.md docs/superpowers/plans/2026-04-07-mpris-applet.md
git commit -m "style: add mpris applet styling"
```
