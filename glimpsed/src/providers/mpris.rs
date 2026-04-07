use std::collections::HashMap;
use std::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::OwnedValue;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};
use crate::providers::dbus_props::DbusPropertyGroup;

const NAME: &str = "mpris";
const TOPICS: &[&str] = &["mpris.current", "mpris.players"];
const METHODS: &[&str] = &[
    "mpris.play_pause",
    "mpris.previous",
    "mpris.next",
    "mpris.raise",
];
const MPRIS_NAME_PREFIX: &str = "org.mpris.MediaPlayer2.";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const MPRIS_ROOT_IFACE: &str = "org.mpris.MediaPlayer2";
const MPRIS_PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const DBUS_SERVICE: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const DBUS_IFACE: &str = "org.freedesktop.DBus";
const MPRIS_PROPERTIES_MATCH_RULE: &str = "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',path='/org/mpris/MediaPlayer2'";

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
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
    players
        .iter()
        .max_by_key(|player| player.last_active)
        .cloned()
}

#[derive(Debug, Default)]
struct ProviderState {
    players: Vec<PlayerSnapshot>,
}

impl ProviderState {
    fn upsert(&mut self, player: PlayerSnapshot) {
        if let Some(existing) = self
            .players
            .iter_mut()
            .find(|existing| existing.player_id == player.player_id)
        {
            *existing = player;
        } else {
            self.players.push(player);
        }

        self.players.sort_by(|a, b| {
            b.last_active
                .cmp(&a.last_active)
                .then_with(|| a.player_id.cmp(&b.player_id))
        });
    }

    fn remove(&mut self, player_id: &str) {
        self.players.retain(|player| player.player_id != player_id);
    }

    fn mark_active(&mut self, player_id: &str, ts: u64) {
        if let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.player_id == player_id)
        {
            player.last_active = ts;
            sort_players(&mut self.players);
        }
    }

    fn current(&self) -> Option<PlayerSnapshot> {
        self.players.first().cloned()
    }
}

fn sort_players(players: &mut [PlayerSnapshot]) {
    players.sort_by(|a, b| {
        b.last_active
            .cmp(&a.last_active)
            .then_with(|| a.player_id.cmp(&b.player_id))
    });
}

fn parse_player_id(params: &serde_json::Value) -> anyhow::Result<&str> {
    params["player_id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'player_id'"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ControlTarget {
    Player,
    Root,
}

fn control_method_target(method: &str) -> anyhow::Result<(ControlTarget, &'static str)> {
    match method {
        "mpris.play_pause" => Ok((ControlTarget::Player, "PlayPause")),
        "mpris.previous" => Ok((ControlTarget::Player, "Previous")),
        "mpris.next" => Ok((ControlTarget::Player, "Next")),
        "mpris.raise" => Ok((ControlTarget::Root, "Raise")),
        _ => Err(anyhow::anyhow!("unknown method: {method}")),
    }
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn player_bus_name(player_id: &str) -> String {
    format!("{MPRIS_NAME_PREFIX}{player_id}")
}

fn strip_player_id(bus_name: &str) -> Option<&str> {
    bus_name.strip_prefix(MPRIS_NAME_PREFIX)
}

fn metadata_string(metadata: &HashMap<String, OwnedValue>, key: &str) -> String {
    metadata
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| String::try_from(value).ok())
        .unwrap_or_default()
}

fn metadata_artists(metadata: &HashMap<String, OwnedValue>) -> String {
    metadata
        .get("xesam:artist")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .map(|artists| artists.join(", "))
        .unwrap_or_default()
}

fn is_state_change_worthy(previous: &PlayerSnapshot, next: &PlayerSnapshot) -> bool {
    previous.status != next.status
        || previous.artist != next.artist
        || previous.track != next.track
        || previous.album != next.album
        || previous.art_url != next.art_url
}

fn refreshed_last_active(previous: Option<&PlayerSnapshot>, next: &PlayerSnapshot, ts: u64) -> u64 {
    match previous {
        Some(previous) if is_state_change_worthy(previous, next) => ts,
        Some(previous) => previous.last_active,
        None if next.status == "Playing" => ts,
        None => 0,
    }
}

fn is_mpris_name(name: &str) -> bool {
    name.starts_with(MPRIS_NAME_PREFIX)
}

fn is_mpris_name_owner_change(change: &zbus::fdo::NameOwnerChanged) -> bool {
    change
        .args()
        .ok()
        .map(|args| is_mpris_name(args.name().as_str()))
        .unwrap_or(false)
}

fn is_mpris_properties_changed(msg: &zbus::message::Message) -> bool {
    let header = msg.header();
    let Some(member) = header.member() else {
        return false;
    };
    if member.as_str() != "PropertiesChanged" {
        return false;
    }

    let Some(interface) = header.interface() else {
        return false;
    };
    if interface.as_str() != "org.freedesktop.DBus.Properties" {
        return false;
    }

    let Some(path) = header.path() else {
        return false;
    };
    if path.as_str() != MPRIS_PATH {
        return false;
    }

    let Ok((changed_iface, changed, invalidated)) =
        msg.body()
            .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
    else {
        return false;
    };

    match changed_iface.as_str() {
        MPRIS_ROOT_IFACE => true,
        MPRIS_PLAYER_IFACE => {
            changed.keys().any(|key| key != "Position")
                || invalidated.iter().any(|key| key != "Position")
        }
        _ => false,
    }
}

async fn emit_snapshot<T: Serialize>(events: &mpsc::Sender<ProviderEvent>, topic: &str, data: &T) {
    let _ = events
        .send(ProviderEvent {
            topic: topic.to_owned(),
            data: serde_json::to_value(data).unwrap_or(serde_json::Value::Null),
        })
        .await;
}

async fn list_mpris_names(conn: &zbus::Connection) -> anyhow::Result<Vec<String>> {
    let proxy = zbus::fdo::DBusProxy::new(conn).await?;
    Ok(proxy
        .list_names()
        .await?
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| is_mpris_name(name))
        .collect())
}

async fn load_player_snapshot(
    conn: &zbus::Connection,
    bus_name: &str,
) -> anyhow::Result<PlayerSnapshot> {
    let player_id = strip_player_id(bus_name)
        .ok_or_else(|| anyhow::anyhow!("invalid MPRIS bus name: {bus_name}"))?
        .to_owned();
    let root = DbusPropertyGroup::new(conn, bus_name, MPRIS_PATH, MPRIS_ROOT_IFACE).await?;
    let player = DbusPropertyGroup::new(conn, bus_name, MPRIS_PATH, MPRIS_PLAYER_IFACE).await?;
    let metadata = player
        .get::<HashMap<String, OwnedValue>>("Metadata")
        .await
        .unwrap_or_default();
    let can_play = player.get::<bool>("CanPlay").await.unwrap_or(false);
    let can_pause = player.get::<bool>("CanPause").await.unwrap_or(false);

    Ok(PlayerSnapshot {
        player_id: player_id.clone(),
        bus_name: bus_name.to_owned(),
        identity: root.get::<String>("Identity").await.unwrap_or(player_id),
        artist: metadata_artists(&metadata),
        track: metadata_string(&metadata, "xesam:title"),
        album: metadata_string(&metadata, "xesam:album"),
        status: player
            .get::<String>("PlaybackStatus")
            .await
            .unwrap_or_else(|| "Stopped".into()),
        art_url: metadata_string(&metadata, "mpris:artUrl"),
        can_go_previous: player.get::<bool>("CanGoPrevious").await.unwrap_or(false),
        can_play_pause: can_play || can_pause,
        can_go_next: player.get::<bool>("CanGoNext").await.unwrap_or(false),
        last_active: 0,
    })
}

async fn call_player_method(
    conn: &zbus::Connection,
    player_id: &str,
    method: &str,
) -> anyhow::Result<serde_json::Value> {
    let player = DbusPropertyGroup::new(
        conn,
        &player_bus_name(player_id),
        MPRIS_PATH,
        MPRIS_PLAYER_IFACE,
    )
    .await?;
    player
        .call_void(method, &())
        .await
        .map(|()| json!(null))
        .map_err(|error| anyhow::anyhow!("{error}"))
}

async fn call_root_method(
    conn: &zbus::Connection,
    player_id: &str,
    method: &str,
) -> anyhow::Result<serde_json::Value> {
    let root = DbusPropertyGroup::new(
        conn,
        &player_bus_name(player_id),
        MPRIS_PATH,
        MPRIS_ROOT_IFACE,
    )
    .await?;
    root.call_void(method, &())
        .await
        .map(|()| json!(null))
        .map_err(|error| anyhow::anyhow!("{error}"))
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

#[derive(Debug, Default)]
pub struct MprisProvider {
    state: ProviderState,
}

impl Provider for MprisProvider {
    fn name(&self) -> &'static str {
        NAME
    }

    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }

    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }

    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("mpris: starting");
            let conn = zbus::Connection::session().await?;
            self.refresh_players(&conn, &events).await?;

            let dbus_proxy = zbus::fdo::DBusProxy::new(&conn).await?;
            let mut name_changes = dbus_proxy.receive_name_owner_changed().await?;
            conn.call_method(
                Some(DBUS_SERVICE),
                DBUS_PATH,
                Some(DBUS_IFACE),
                "AddMatch",
                &(MPRIS_PROPERTIES_MATCH_RULE,),
            )
            .await?;
            let mut prop_changes = zbus::MessageStream::from(&conn);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req, Some(&conn), Some(&events)).await;
                    }
                    Some(change) = name_changes.next() => {
                        if is_mpris_name_owner_change(&change) {
                            self.refresh_players(&conn, &events).await?;
                        }
                    }
                    Some(Ok(msg)) = prop_changes.next() => {
                        if is_mpris_properties_changed(&msg) {
                            self.refresh_players(&conn, &events).await?;
                        }
                    }
                }
            }

            tracing::info!("mpris: stopping");
            Ok(())
        })
    }
}

impl MprisProvider {
    async fn refresh_players(
        &mut self,
        conn: &zbus::Connection,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> anyhow::Result<()> {
        let previous_players = self.state.players.clone();
        let names = list_mpris_names(conn).await?;

        self.state
            .players
            .retain(|player| names.iter().any(|name| name == &player.bus_name));

        let ts = now_ts();
        for bus_name in names {
            let mut snapshot = match load_player_snapshot(conn, &bus_name).await {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    tracing::warn!(bus_name = %bus_name, error = %error, "mpris: failed to refresh player");
                    continue;
                }
            };
            let previous = previous_players
                .iter()
                .find(|player| player.player_id == snapshot.player_id);
            snapshot.last_active = refreshed_last_active(previous, &snapshot, ts);
            self.state.upsert(snapshot);
        }

        if self.state.players != previous_players {
            self.emit_state(events).await;
        }

        Ok(())
    }

    async fn emit_state(&self, events: &mpsc::Sender<ProviderEvent>) {
        emit_snapshot(events, "mpris.players", &self.state.players).await;
        emit_snapshot(events, "mpris.current", &self.state.current()).await;
    }

    async fn dispatch_call(
        &mut self,
        method: &str,
        params: &serde_json::Value,
        conn: &zbus::Connection,
        events: Option<&mpsc::Sender<ProviderEvent>>,
    ) -> anyhow::Result<serde_json::Value> {
        let player_id = parse_player_id(params)?;
        let result = match control_method_target(method)? {
            (ControlTarget::Player, dbus_method) => {
                call_player_method(conn, player_id, dbus_method).await
            }
            (ControlTarget::Root, dbus_method) => call_root_method(conn, player_id, dbus_method).await,
        }?;

        self.state.mark_active(player_id, now_ts());
        if let Some(events) = events {
            self.emit_state(events).await;
        }

        Ok(result)
    }

    async fn handle_request(
        &mut self,
        req: ProviderRequest,
        conn: Option<&zbus::Connection>,
        events: Option<&mpsc::Sender<ProviderEvent>>,
    ) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "mpris.current" => serde_json::to_value(self.state.current()).ok(),
                    "mpris.players" => serde_json::to_value(&self.state.players).ok(),
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call {
                method,
                params,
                reply,
            } => {
                let result = match conn {
                    Some(conn) => self.dispatch_call(&method, &params, conn, events).await,
                    None => Err(anyhow::anyhow!("not implemented")),
                };
                if let Err(ref error) = result {
                    tracing::warn!(method = %method, error = %error, "mpris: call failed");
                }
                let _ = reply.send(result);
            }
        }
    }
}

pub struct MprisProviderFactory;

impl ProviderFactory for MprisProviderFactory {
    fn name(&self) -> &'static str {
        NAME
    }

    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }

    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }

    fn create(&self) -> Box<dyn Provider> {
        Box::new(MprisProvider::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_metadata() {
        assert_eq!(NAME, "mpris");
        assert_eq!(TOPICS, &["mpris.current", "mpris.players"]);
        assert_eq!(
            METHODS,
            &[
                "mpris.play_pause",
                "mpris.previous",
                "mpris.next",
                "mpris.raise"
            ]
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

    #[tokio::test]
    async fn current_snapshot_returns_null() {
        let mut provider = MprisProvider::default();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        provider
            .handle_request(
                ProviderRequest::Snapshot {
                    topic: "mpris.current".into(),
                    reply: reply_tx,
                },
                None,
                None,
            )
            .await;

        assert_eq!(reply_rx.await.unwrap(), Some(serde_json::Value::Null));
    }

    #[tokio::test]
    async fn players_snapshot_returns_empty_array() {
        let mut provider = MprisProvider::default();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        provider
            .handle_request(
                ProviderRequest::Snapshot {
                    topic: "mpris.players".into(),
                    reply: reply_tx,
                },
                None,
                None,
            )
            .await;

        assert_eq!(reply_rx.await.unwrap(), Some(serde_json::json!([])));
    }

    #[tokio::test]
    async fn methods_return_not_implemented() {
        let mut provider = MprisProvider::default();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        provider
            .handle_request(
                ProviderRequest::Call {
                    method: "mpris.play_pause".into(),
                    params: serde_json::json!({}),
                    reply: reply_tx,
                },
                None,
                None,
            )
            .await;

        let err = reply_rx.await.unwrap().expect_err("expected error");
        assert_eq!(err.to_string(), "not implemented");
    }

    #[test]
    fn subtitle_falls_back_from_artist_and_album_to_identity() {
        let player = PlayerSnapshot {
            identity: "Firefox".into(),
            artist: String::new(),
            album: String::new(),
            ..PlayerSnapshot::test_default()
        };

        assert_eq!(player.subtitle(), "Firefox");
    }

    #[test]
    fn panel_label_falls_back_from_missing_artist_and_track() {
        let player = PlayerSnapshot {
            identity: "Firefox".into(),
            artist: String::new(),
            track: String::new(),
            ..PlayerSnapshot::test_default()
        };

        assert_eq!(player.panel_label("{artist} - {track}"), "Firefox");
    }

    #[test]
    fn provider_state_orders_and_replaces_players() {
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
        state.upsert(PlayerSnapshot {
            player_id: "spotify".into(),
            last_active: 30,
            identity: "Spotify".into(),
            ..PlayerSnapshot::test_default()
        });

        assert_eq!(state.players.len(), 2);
        assert_eq!(state.players[0].player_id, "spotify");
        assert_eq!(state.current().unwrap().player_id, "spotify");
        assert_eq!(state.current().unwrap().last_active, 30);
        assert_eq!(state.players[1].player_id, "firefox");
    }

    #[tokio::test]
    async fn snapshot_replies_use_provider_state() {
        let mut state = ProviderState::default();
        state.upsert(PlayerSnapshot {
            player_id: "spotify".into(),
            identity: "Spotify".into(),
            last_active: 10,
            ..PlayerSnapshot::test_default()
        });
        state.upsert(PlayerSnapshot {
            player_id: "firefox".into(),
            identity: "Firefox".into(),
            last_active: 20,
            ..PlayerSnapshot::test_default()
        });
        let mut provider = MprisProvider { state };

        let (current_reply_tx, current_reply_rx) = tokio::sync::oneshot::channel();
        provider
            .handle_request(
                ProviderRequest::Snapshot {
                    topic: "mpris.current".into(),
                    reply: current_reply_tx,
                },
                None,
                None,
            )
            .await;

        let current = current_reply_rx.await.unwrap().expect("current snapshot");
        let (players_reply_tx, players_reply_rx) = tokio::sync::oneshot::channel();
        provider
            .handle_request(
                ProviderRequest::Snapshot {
                    topic: "mpris.players".into(),
                    reply: players_reply_tx,
                },
                None,
                None,
            )
            .await;

        let players = players_reply_rx.await.unwrap().expect("players snapshot");
        assert_eq!(players.as_array().unwrap().len(), 2);
        assert_eq!(players[0]["player_id"], "firefox");
        assert_eq!(current["player_id"], players[0]["player_id"]);
        assert_eq!(current["player_id"], "firefox");
        assert_eq!(players[1]["player_id"], "spotify");
    }

    #[test]
    fn provider_state_remove_recomputes_current_and_handles_empty_state() {
        let mut state = ProviderState::default();
        state.upsert(PlayerSnapshot {
            player_id: "spotify".into(),
            last_active: 20,
            ..PlayerSnapshot::test_default()
        });
        state.upsert(PlayerSnapshot {
            player_id: "firefox".into(),
            last_active: 20,
            ..PlayerSnapshot::test_default()
        });

        assert_eq!(state.players[0].player_id, "firefox");
        assert_eq!(state.current().unwrap().player_id, "firefox");

        state.remove("firefox");

        assert_eq!(state.players.len(), 1);
        assert_eq!(state.players[0].player_id, "spotify");
        assert_eq!(state.current().unwrap().player_id, "spotify");

        state.remove("spotify");

        assert!(state.players.is_empty());
        assert!(state.current().is_none());
    }

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
        let err = parse_player_id(&serde_json::json!({}))
            .unwrap_err()
            .to_string();
        assert!(err.contains("missing 'player_id'"));
    }

    #[test]
    fn control_methods_map_to_expected_dbus_methods() {
        assert_eq!(
            control_method_target("mpris.play_pause").unwrap(),
            (ControlTarget::Player, "PlayPause")
        );
        assert_eq!(
            control_method_target("mpris.previous").unwrap(),
            (ControlTarget::Player, "Previous")
        );
        assert_eq!(
            control_method_target("mpris.next").unwrap(),
            (ControlTarget::Player, "Next")
        );
        assert_eq!(
            control_method_target("mpris.raise").unwrap(),
            (ControlTarget::Root, "Raise")
        );
    }
}
