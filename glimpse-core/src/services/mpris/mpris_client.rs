use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::{
    MatchRule, MessageStream,
    message::{Message, Type},
    zvariant::OwnedValue,
};

use crate::{
    dbus::mpris::{
        MPRIS_NAME_PREFIX, MPRIS_PATH, MPRIS_PLAYER_INTERFACE, MPRIS_ROOT_INTERFACE,
        MprisPlayerProxy, MprisRootProxy,
    },
    services::mpris::model::{Artwork, PlaybackStatus, Player, Snapshot},
};

const DBUS_PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const POSITION_PROPERTY: &str = "Position";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MprisChangeReason {
    NameOwnerChanged,
    PropertiesChanged,
    Seeked,
}

impl fmt::Display for MprisChangeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NameOwnerChanged => "name-owner-changed",
            Self::PropertiesChanged => "properties-changed",
            Self::Seeked => "seeked",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisClientEvent {
    Changed { reason: MprisChangeReason },
}

#[derive(Clone)]
pub struct MprisClient {
    session: zbus::Connection,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl MprisClient {
    pub async fn new(session: zbus::Connection) -> anyhow::Result<Self> {
        zbus::fdo::DBusProxy::new(&session)
            .await
            .context("failed to create DBus proxy")?;
        Ok(Self {
            session,
            snapshot: Arc::new(Mutex::new(Snapshot::default())),
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        lock_snapshot(&self.snapshot).clone()
    }

    pub async fn refresh(&self) -> anyhow::Result<Snapshot> {
        let previous = self.snapshot();
        let names = list_mpris_names(&self.session).await?;
        let timestamp = now_secs();
        let mut players = Vec::with_capacity(names.len());

        for bus_name in names {
            match load_player(&self.session, &bus_name, &previous.players, timestamp).await {
                Ok(player) => players.push(player),
                Err(error) => {
                    tracing::debug!(
                        bus_name = %bus_name,
                        error = %error,
                        "mpris: skipped player during refresh"
                    );
                }
            }
        }

        sort_players(&mut players);
        let snapshot = Snapshot {
            current_player: select_current_player(&players),
            players,
        };
        *lock_snapshot(&self.snapshot) = snapshot.clone();
        Ok(snapshot)
    }

    pub async fn refresh_positions(&self) -> anyhow::Result<Snapshot> {
        let mut snapshot = self.snapshot();
        let mut changed = false;

        for player in snapshot.players.iter_mut().filter(|player| {
            player.playback_status == PlaybackStatus::Playing && player.progress_visible
        }) {
            let position = match player_proxy(&self.session, &player.player_id).await {
                Ok(proxy) => match proxy.position().await {
                    Ok(raw) => normalize_microseconds(raw),
                    Err(error) => {
                        tracing::debug!(
                            player_id = %player.player_id,
                            error = %error,
                            "mpris: skipped position refresh"
                        );
                        continue;
                    }
                },
                Err(error) => {
                    tracing::debug!(
                        player_id = %player.player_id,
                        error = %error,
                        "mpris: skipped position refresh"
                    );
                    continue;
                }
            };

            if player.position != position {
                player.position = position;
                changed = true;
            }
        }

        if changed {
            snapshot.current_player = select_current_player(&snapshot.players);
            *lock_snapshot(&self.snapshot) = snapshot.clone();
        }

        Ok(snapshot)
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<MprisClientEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let dbus = zbus::fdo::DBusProxy::new(&self.session).await?;
        let mut name_changes = dbus.receive_name_owner_changed().await?;
        let mut property_changes =
            MessageStream::for_match_rule(mpris_properties_match_rule()?, &self.session, None)
                .await?;
        let mut seeked =
            MessageStream::for_match_rule(mpris_seeked_match_rule()?, &self.session, None).await?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                maybe_change = name_changes.next() => {
                    match maybe_change {
                        Some(change) if is_mpris_name_owner_change(&change) => {
                            if events.send(MprisClientEvent::Changed {
                                reason: MprisChangeReason::NameOwnerChanged,
                            }).await.is_err() {
                                return Ok(());
                            }
                        }
                        Some(_) => {}
                        None => return Ok(()),
                    }
                }
                maybe_message = property_changes.next() => {
                    match maybe_message {
                        Some(Ok(message)) if is_mpris_properties_changed(&message) => {
                            if events.send(MprisClientEvent::Changed {
                                reason: MprisChangeReason::PropertiesChanged,
                            }).await.is_err() {
                                return Ok(());
                            }
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => {
                            tracing::debug!(error = %error, "mpris: property stream error");
                        }
                        None => return Ok(()),
                    }
                }
                maybe_message = seeked.next() => {
                    match maybe_message {
                        Some(Ok(message)) if is_mpris_seeked(&message) => {
                            if events.send(MprisClientEvent::Changed {
                                reason: MprisChangeReason::Seeked,
                            }).await.is_err() {
                                return Ok(());
                            }
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => {
                            tracing::debug!(error = %error, "mpris: seeked stream error");
                        }
                        None => return Ok(()),
                    }
                }
            }
        }
    }

    pub async fn play_pause(&self, player_id: &str) -> anyhow::Result<()> {
        player_proxy(&self.session, player_id)
            .await?
            .play_pause()
            .await?;
        self.mark_player_active(player_id);
        Ok(())
    }

    pub async fn previous(&self, player_id: &str) -> anyhow::Result<()> {
        player_proxy(&self.session, player_id)
            .await?
            .previous()
            .await?;
        self.mark_player_active(player_id);
        Ok(())
    }

    pub async fn next(&self, player_id: &str) -> anyhow::Result<()> {
        player_proxy(&self.session, player_id).await?.next().await?;
        self.mark_player_active(player_id);
        Ok(())
    }

    pub async fn raise(&self, player_id: &str) -> anyhow::Result<()> {
        root_proxy(&self.session, player_id).await?.raise().await?;
        self.mark_player_active(player_id);
        Ok(())
    }

    fn mark_player_active(&self, player_id: &str) {
        let mut snapshot = lock_snapshot(&self.snapshot);
        if let Some(player) = snapshot
            .players
            .iter_mut()
            .find(|player| player.player_id == player_id)
        {
            player.last_active = now_secs();
        }
        sort_players(&mut snapshot.players);
        snapshot.current_player = select_current_player(&snapshot.players);
    }
}

fn lock_snapshot(snapshot: &Arc<Mutex<Snapshot>>) -> std::sync::MutexGuard<'_, Snapshot> {
    snapshot.lock().unwrap_or_else(|poison| poison.into_inner())
}

pub fn playback_status_from_raw(raw: &str) -> PlaybackStatus {
    if raw.trim().eq_ignore_ascii_case("playing") {
        PlaybackStatus::Playing
    } else if raw.trim().eq_ignore_ascii_case("paused") {
        PlaybackStatus::Paused
    } else {
        PlaybackStatus::Stopped
    }
}

pub fn artwork_from_raw(raw: &str) -> Artwork {
    let raw = raw.trim();
    if raw.is_empty() {
        return Artwork::None;
    }
    if raw.starts_with("file://") {
        return Artwork::FileUri(raw.into());
    }
    if raw.starts_with("http://") || raw.starts_with("https://") || raw.contains("://") {
        return Artwork::RemoteUrl(raw.into());
    }
    Artwork::FilePath(raw.into())
}

pub fn subtitle_for(artist: &str, album: &str, identity: &str) -> String {
    if !artist.is_empty() {
        artist.into()
    } else if !album.is_empty() {
        album.into()
    } else {
        identity.into()
    }
}

pub fn select_current_player(players: &[Player]) -> Option<Player> {
    players
        .iter()
        .max_by(|left, right| {
            status_rank(left.playback_status)
                .cmp(&status_rank(right.playback_status))
                .then_with(|| left.last_active.cmp(&right.last_active))
                .then_with(|| right.player_id.cmp(&left.player_id))
        })
        .cloned()
}

fn sort_players(players: &mut [Player]) {
    players.sort_by(|left, right| {
        status_rank(right.playback_status)
            .cmp(&status_rank(left.playback_status))
            .then_with(|| right.last_active.cmp(&left.last_active))
            .then_with(|| left.player_id.cmp(&right.player_id))
    });
}

fn status_rank(status: PlaybackStatus) -> u8 {
    match status {
        PlaybackStatus::Playing => 2,
        PlaybackStatus::Paused => 1,
        PlaybackStatus::Stopped => 0,
    }
}

fn player_bus_name(player_id: &str) -> String {
    format!("{MPRIS_NAME_PREFIX}{player_id}")
}

fn mpris_properties_match_rule() -> anyhow::Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(DBUS_PROPERTIES_INTERFACE)?
        .member("PropertiesChanged")?
        .path(MPRIS_PATH)?
        .build()
        .into_owned())
}

fn mpris_seeked_match_rule() -> anyhow::Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(MPRIS_PLAYER_INTERFACE)?
        .member("Seeked")?
        .path(MPRIS_PATH)?
        .build()
        .into_owned())
}

async fn list_mpris_names(conn: &zbus::Connection) -> anyhow::Result<Vec<String>> {
    let proxy = zbus::fdo::DBusProxy::new(conn).await?;
    let mut names = proxy
        .list_names()
        .await?
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| name.starts_with(MPRIS_NAME_PREFIX))
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

async fn load_player(
    session: &zbus::Connection,
    bus_name: &str,
    previous_players: &[Player],
    timestamp: u64,
) -> anyhow::Result<Player> {
    let player_id = bus_name
        .strip_prefix(MPRIS_NAME_PREFIX)
        .ok_or_else(|| anyhow::anyhow!("invalid MPRIS bus name: {bus_name}"))?
        .to_owned();
    let root = MprisRootProxy::builder(session)
        .destination(bus_name.to_owned())?
        .path(MPRIS_PATH)?
        .build()
        .await?;
    let player = MprisPlayerProxy::builder(session)
        .destination(bus_name.to_owned())?
        .path(MPRIS_PATH)?
        .uncached_properties(uncached_player_properties())
        .build()
        .await?;

    let metadata = player.metadata().await.unwrap_or_default();
    let identity = root.identity().await.unwrap_or_else(|_| player_id.clone());
    let artist = metadata_artists(&metadata);
    let title = metadata_string(&metadata, "xesam:title");
    let album = metadata_string(&metadata, "xesam:album");
    let playback_status = playback_status_from_raw(
        &player
            .playback_status()
            .await
            .unwrap_or_else(|_| "Stopped".into()),
    );
    let artwork = artwork_from_raw(&metadata_string(&metadata, "mpris:artUrl"));
    let position = player
        .position()
        .await
        .ok()
        .and_then(normalize_microseconds);
    let length = metadata_microseconds(&metadata, "mpris:length");
    let previous = previous_players
        .iter()
        .find(|candidate| candidate.player_id == player_id);
    let next = Player {
        player_id: player_id.clone(),
        bus_name: bus_name.into(),
        identity: identity.clone(),
        playback_status,
        title: title.clone(),
        artist: artist.clone(),
        album: album.clone(),
        subtitle: subtitle_for(&artist, &album, &identity),
        artwork,
        position,
        length,
        progress_visible: matches!((position, length), (Some(_), Some(length)) if length > 0),
        can_play_pause: player.can_play().await.unwrap_or(false)
            || player.can_pause().await.unwrap_or(false),
        can_go_previous: player.can_go_previous().await.unwrap_or(false),
        can_go_next: player.can_go_next().await.unwrap_or(false),
        can_raise: root.can_raise().await.unwrap_or(false),
        last_active: 0,
    };

    Ok(Player {
        last_active: refreshed_last_active(previous, &next, timestamp),
        ..next
    })
}

async fn player_proxy<'a>(
    session: &'a zbus::Connection,
    player_id: &str,
) -> anyhow::Result<MprisPlayerProxy<'a>> {
    Ok(MprisPlayerProxy::builder(session)
        .destination(player_bus_name(player_id))?
        .path(MPRIS_PATH)?
        .uncached_properties(uncached_player_properties())
        .build()
        .await?)
}

fn uncached_player_properties() -> &'static [&'static str] {
    &[POSITION_PROPERTY]
}

async fn root_proxy<'a>(
    session: &'a zbus::Connection,
    player_id: &str,
) -> anyhow::Result<MprisRootProxy<'a>> {
    Ok(MprisRootProxy::builder(session)
        .destination(player_bus_name(player_id))?
        .path(MPRIS_PATH)?
        .build()
        .await?)
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

fn metadata_microseconds(metadata: &HashMap<String, OwnedValue>, key: &str) -> Option<u64> {
    let value = metadata.get(key)?.try_clone().ok()?;
    i64::try_from(value.clone())
        .ok()
        .and_then(normalize_microseconds)
        .or_else(|| {
            u64::try_from(value)
                .ok()
                .and_then(|value| i64::try_from(value).ok())
                .and_then(normalize_microseconds)
        })
}

fn normalize_microseconds(value: i64) -> Option<u64> {
    u64::try_from(value).ok()
}

fn is_meaningful_change(previous: &Player, next: &Player) -> bool {
    previous.playback_status != next.playback_status
        || previous.title != next.title
        || previous.artist != next.artist
        || previous.album != next.album
        || previous.artwork != next.artwork
}

fn refreshed_last_active(previous: Option<&Player>, next: &Player, timestamp: u64) -> u64 {
    match previous {
        Some(previous) if is_meaningful_change(previous, next) => timestamp,
        Some(previous) => previous.last_active,
        None if matches!(next.playback_status, PlaybackStatus::Playing) => timestamp,
        None => 0,
    }
}

fn is_mpris_name_owner_change(change: &zbus::fdo::NameOwnerChanged) -> bool {
    change
        .args()
        .ok()
        .map(|args| args.name().as_str().starts_with(MPRIS_NAME_PREFIX))
        .unwrap_or(false)
}

fn is_mpris_properties_changed(message: &Message) -> bool {
    let header = message.header();
    let Some(member) = header.member() else {
        return false;
    };
    if member.as_str() != "PropertiesChanged" {
        return false;
    }

    let Some(interface) = header.interface() else {
        return false;
    };
    if interface.as_str() != DBUS_PROPERTIES_INTERFACE {
        return false;
    }

    let Some(path) = header.path() else {
        return false;
    };
    if path.as_str() != MPRIS_PATH {
        return false;
    }

    let Ok((changed_interface, changed, invalidated)) =
        message
            .body()
            .deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
    else {
        return false;
    };

    match changed_interface.as_str() {
        MPRIS_ROOT_INTERFACE => true,
        MPRIS_PLAYER_INTERFACE => {
            changed.keys().any(|key| key != "Position")
                || invalidated.iter().any(|key| key != "Position")
        }
        _ => false,
    }
}

fn is_mpris_seeked(message: &Message) -> bool {
    let header = message.header();
    let Some(member) = header.member() else {
        return false;
    };
    if member.as_str() != "Seeked" {
        return false;
    }

    let Some(interface) = header.interface() else {
        return false;
    };
    if interface.as_str() != MPRIS_PLAYER_INTERFACE {
        return false;
    }

    let Some(path) = header.path() else {
        return false;
    };

    path.as_str() == MPRIS_PATH
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::match_rule::PathSpec;

    fn player(id: &str, status: PlaybackStatus, last_active: u64) -> Player {
        Player {
            player_id: id.into(),
            bus_name: format!("{MPRIS_NAME_PREFIX}{id}"),
            identity: id.into(),
            playback_status: status,
            title: id.into(),
            subtitle: id.into(),
            can_play_pause: true,
            last_active,
            ..Default::default()
        }
    }

    #[test]
    fn maps_raw_playback_status_into_typed_status() {
        assert_eq!(playback_status_from_raw("Playing"), PlaybackStatus::Playing);
        assert_eq!(playback_status_from_raw("Paused"), PlaybackStatus::Paused);
        assert_eq!(playback_status_from_raw("Stopped"), PlaybackStatus::Stopped);
        assert_eq!(playback_status_from_raw("unknown"), PlaybackStatus::Stopped);
    }

    #[test]
    fn maps_artwork_sources() {
        assert_eq!(artwork_from_raw(""), Artwork::None);
        assert_eq!(
            artwork_from_raw("file:///tmp/cover.png"),
            Artwork::FileUri("file:///tmp/cover.png".into())
        );
        assert_eq!(
            artwork_from_raw("https://example.test/cover.png"),
            Artwork::RemoteUrl("https://example.test/cover.png".into())
        );
        assert_eq!(
            artwork_from_raw("/tmp/cover.png"),
            Artwork::FilePath("/tmp/cover.png".into())
        );
    }

    #[test]
    fn current_player_prefers_playing_then_recent_then_stable_id() {
        let players = vec![
            player("spotify", PlaybackStatus::Paused, 30),
            player("mpv", PlaybackStatus::Playing, 10),
            player("firefox", PlaybackStatus::Playing, 20),
        ];

        assert_eq!(
            select_current_player(&players).map(|player| player.player_id),
            Some("firefox".into())
        );
    }

    #[test]
    fn properties_match_rule_is_scoped_to_mpris_property_signals() {
        let rule = mpris_properties_match_rule().expect("valid MPRIS match rule");

        assert_eq!(rule.msg_type(), Some(Type::Signal));
        assert_eq!(
            rule.interface().expect("interface").as_str(),
            DBUS_PROPERTIES_INTERFACE
        );
        assert_eq!(rule.member().expect("member").as_str(), "PropertiesChanged");

        match rule.path_spec().expect("path spec") {
            PathSpec::Path(path) => assert_eq!(path.as_str(), MPRIS_PATH),
            PathSpec::PathNamespace(path) => {
                panic!("expected exact path match, got namespace {}", path.as_str())
            }
        }
    }

    #[test]
    fn seeked_match_rule_is_scoped_to_mpris_player_seeked_signals() {
        let rule = mpris_seeked_match_rule().expect("valid MPRIS seeked match rule");

        assert_eq!(rule.msg_type(), Some(Type::Signal));
        assert_eq!(
            rule.interface().expect("interface").as_str(),
            MPRIS_PLAYER_INTERFACE
        );
        assert_eq!(rule.member().expect("member").as_str(), "Seeked");

        match rule.path_spec().expect("path spec") {
            PathSpec::Path(path) => assert_eq!(path.as_str(), MPRIS_PATH),
            PathSpec::PathNamespace(path) => {
                panic!("expected exact path match, got namespace {}", path.as_str())
            }
        }
    }

    #[test]
    fn player_proxy_keeps_position_uncached_for_live_progress() {
        assert_eq!(uncached_player_properties(), &[POSITION_PROPERTY]);
    }
}
