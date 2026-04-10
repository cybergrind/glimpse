use std::{
    collections::HashMap,
    fmt,
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::{MatchRule, MessageStream, message::{Message, Type}, zvariant::OwnedValue};

use crate::{
    dbus::mpris::{
        MPRIS_NAME_PREFIX, MPRIS_PATH, MPRIS_PLAYER_INTERFACE, MPRIS_ROOT_INTERFACE,
        MprisPlayerProxy, MprisRootProxy,
    },
    mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer, MprisSnapshot},
};

const DBUS_PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MprisChangeReason {
    NameOwnerChanged,
    PropertiesChanged,
}

impl fmt::Display for MprisChangeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NameOwnerChanged => "name-owner-changed",
            Self::PropertiesChanged => "properties-changed",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisProviderEvent {
    Changed { reason: MprisChangeReason },
}

#[derive(Clone)]
pub struct MprisProvider {
    session: zbus::Connection,
    snapshot: Arc<Mutex<MprisSnapshot>>,
}

impl MprisProvider {
    pub async fn new(session: zbus::Connection) -> anyhow::Result<Self> {
        let _ = zbus::fdo::DBusProxy::new(&session).await?;
        Ok(Self {
            session,
            snapshot: Arc::new(Mutex::new(MprisSnapshot::default())),
        })
    }

    pub fn snapshot(&self) -> MprisSnapshot {
        self.snapshot
            .lock()
            .expect("mpris provider snapshot poisoned")
            .clone()
    }

    pub async fn refresh(&self) -> anyhow::Result<()> {
        let previous = self.snapshot();
        let names = list_mpris_names(&self.session).await?;
        let timestamp = now_ts();
        let mut players = Vec::with_capacity(names.len());

        for bus_name in names {
            match load_player(&self.session, &bus_name, &previous.players, timestamp).await {
                Ok(player) => players.push(player),
                Err(error) => {
                    tracing::warn!(bus_name = %bus_name, error = %error, "mpris provider: failed to refresh player");
                }
            }
        }

        sort_players(&mut players);
        let snapshot = MprisSnapshot {
            current_player: players.first().cloned(),
            players,
        };
        *self
            .snapshot
            .lock()
            .expect("mpris provider snapshot poisoned") = snapshot;
        Ok(())
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<MprisProviderEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let dbus = zbus::fdo::DBusProxy::new(&self.session).await?;
        let mut name_changes = dbus.receive_name_owner_changed().await?;
        let mut property_changes =
            MessageStream::for_match_rule(mpris_properties_match_rule()?, &self.session, None)
                .await?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                maybe_change = name_changes.next() => {
                    match maybe_change {
                        Some(change) if is_mpris_name_owner_change(&change) => {
                            if events.send(MprisProviderEvent::Changed {
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
                            if events.send(MprisProviderEvent::Changed {
                                reason: MprisChangeReason::PropertiesChanged,
                            }).await.is_err() {
                                return Ok(());
                            }
                        }
                        Some(Ok(_)) => {}
                        Some(Err(error)) => {
                            tracing::warn!(error = %error, "mpris provider: properties stream error");
                        }
                        None => return Ok(()),
                    }
                }
            }
        }
    }

    pub async fn play_pause(&self, player_id: &str) -> anyhow::Result<()> {
        let proxy = player_proxy(&self.session, player_id).await?;
        proxy.play_pause().await?;
        self.mark_player_active(player_id);
        Ok(())
    }

    pub async fn previous(&self, player_id: &str) -> anyhow::Result<()> {
        let proxy = player_proxy(&self.session, player_id).await?;
        proxy.previous().await?;
        self.mark_player_active(player_id);
        Ok(())
    }

    pub async fn next(&self, player_id: &str) -> anyhow::Result<()> {
        let proxy = player_proxy(&self.session, player_id).await?;
        proxy.next().await?;
        self.mark_player_active(player_id);
        Ok(())
    }

    pub async fn raise(&self, player_id: &str) -> anyhow::Result<()> {
        let proxy = root_proxy(&self.session, player_id).await?;
        proxy.raise().await?;
        self.mark_player_active(player_id);
        Ok(())
    }

    fn mark_player_active(&self, player_id: &str) {
        let mut snapshot = self
            .snapshot
            .lock()
            .expect("mpris provider snapshot poisoned");
        if let Some(player) = snapshot
            .players
            .iter_mut()
            .find(|player| player.player_id == player_id)
        {
            player.last_active = now_ts();
        }
        sort_players(&mut snapshot.players);
        snapshot.current_player = snapshot.players.first().cloned();
    }
}

pub fn playback_status_from_raw(raw: &str) -> MprisPlaybackStatus {
    let raw = raw.trim();

    if raw.eq_ignore_ascii_case("playing") {
        MprisPlaybackStatus::Playing
    } else if raw.eq_ignore_ascii_case("paused") {
        MprisPlaybackStatus::Paused
    } else {
        MprisPlaybackStatus::Stopped
    }
}

pub fn artwork_from_raw(raw: &str) -> MprisArtwork {
    let raw = raw.trim();

    if raw.is_empty() {
        return MprisArtwork::None;
    }

    if raw.starts_with("file://") {
        return MprisArtwork::FileUri(raw.to_owned());
    }

    if raw.starts_with("http://") || raw.starts_with("https://") || raw.contains("://") {
        return MprisArtwork::RemoteUrl(raw.to_owned());
    }

    if Path::new(raw).is_absolute() {
        return MprisArtwork::FilePath(raw.to_owned());
    }

    MprisArtwork::FilePath(raw.to_owned())
}

pub fn subtitle_for(artist: &str, album: &str, identity: &str) -> String {
    if !artist.is_empty() {
        artist.to_owned()
    } else if !album.is_empty() {
        album.to_owned()
    } else {
        identity.to_owned()
    }
}

pub fn panel_label_for(artist: &str, title: &str, identity: &str) -> String {
    if !artist.is_empty() && !title.is_empty() {
        format!("{artist} - {title}")
    } else if !title.is_empty() {
        title.to_owned()
    } else {
        identity.to_owned()
    }
}

pub fn select_current_player(players: &[MprisPlayer]) -> Option<MprisPlayer> {
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

fn status_rank(status: MprisPlaybackStatus) -> u8 {
    match status {
        MprisPlaybackStatus::Playing => 2,
        MprisPlaybackStatus::Paused => 1,
        MprisPlaybackStatus::Stopped => 0,
    }
}

fn sort_players(players: &mut [MprisPlayer]) {
    players.sort_by(|left, right| {
        status_rank(right.playback_status)
            .cmp(&status_rank(left.playback_status))
            .then_with(|| right.last_active.cmp(&left.last_active))
            .then_with(|| left.player_id.cmp(&right.player_id))
    });
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

fn mpris_properties_match_rule() -> anyhow::Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .interface(DBUS_PROPERTIES_INTERFACE)?
        .member("PropertiesChanged")?
        .path(MPRIS_PATH)?
        .build()
        .into_owned())
}

async fn list_mpris_names(conn: &zbus::Connection) -> anyhow::Result<Vec<String>> {
    let proxy = zbus::fdo::DBusProxy::new(conn).await?;
    Ok(proxy
        .list_names()
        .await?
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| name.starts_with(MPRIS_NAME_PREFIX))
        .collect())
}

async fn load_player(
    session: &zbus::Connection,
    bus_name: &str,
    previous_players: &[MprisPlayer],
    timestamp: u64,
) -> anyhow::Result<MprisPlayer> {
    let player_id = strip_player_id(bus_name)
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
    let next = MprisPlayer {
        player_id: player_id.clone(),
        bus_name: bus_name.to_owned(),
        identity: identity.clone(),
        playback_status,
        title: title.clone(),
        artist: artist.clone(),
        album: album.clone(),
        panel_label: panel_label_for(&artist, &title, &identity),
        subtitle: subtitle_for(&artist, &album, &identity),
        artwork: artwork.clone(),
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

    Ok(MprisPlayer {
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
        .build()
        .await?)
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

fn is_meaningful_change(previous: &MprisPlayer, next: &MprisPlayer) -> bool {
    previous.playback_status != next.playback_status
        || previous.title != next.title
        || previous.artist != next.artist
        || previous.album != next.album
        || previous.artwork != next.artwork
}

fn refreshed_last_active(
    previous: Option<&MprisPlayer>,
    next: &MprisPlayer,
    timestamp: u64,
) -> u64 {
    match previous {
        Some(previous) if is_meaningful_change(previous, next) => timestamp,
        Some(previous) => previous.last_active,
        None if matches!(next.playback_status, MprisPlaybackStatus::Playing) => timestamp,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};
    use zbus::match_rule::PathSpec;

    #[test]
    fn properties_match_rule_is_scoped_to_mpris_property_signals() {
        let rule = mpris_properties_match_rule().expect("valid static match rule");

        assert_eq!(rule.msg_type(), Some(Type::Signal));
        assert_eq!(rule.interface().expect("interface").as_str(), DBUS_PROPERTIES_INTERFACE);
        assert_eq!(rule.member().expect("member").as_str(), "PropertiesChanged");
        assert_eq!(rule.sender(), None);

        match rule.path_spec().expect("path spec") {
            PathSpec::Path(path) => assert_eq!(path.as_str(), MPRIS_PATH),
            PathSpec::PathNamespace(path) => {
                panic!("expected exact path match, got namespace {}", path.as_str())
            }
        }
    }

    fn player(id: &str, status: MprisPlaybackStatus, last_active: u64) -> MprisPlayer {
        MprisPlayer {
            player_id: id.into(),
            bus_name: format!("org.mpris.MediaPlayer2.{id}"),
            identity: id.into(),
            playback_status: status,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            panel_label: id.into(),
            subtitle: id.into(),
            artwork: MprisArtwork::None,
            position: None,
            length: None,
            progress_visible: false,
            can_play_pause: true,
            can_go_previous: false,
            can_go_next: false,
            can_raise: false,
            last_active,
        }
    }

    #[test]
    fn maps_raw_playback_status_into_typed_status() {
        assert_eq!(
            playback_status_from_raw("Playing"),
            MprisPlaybackStatus::Playing
        );
        assert_eq!(
            playback_status_from_raw("Paused"),
            MprisPlaybackStatus::Paused
        );
        assert_eq!(
            playback_status_from_raw("Stopped"),
            MprisPlaybackStatus::Stopped
        );
        assert_eq!(
            playback_status_from_raw(" playing "),
            MprisPlaybackStatus::Playing
        );
        assert_eq!(
            playback_status_from_raw("UnknownStatus"),
            MprisPlaybackStatus::Stopped
        );
    }

    #[test]
    fn maps_raw_artwork_values_into_typed_artwork() {
        assert_eq!(artwork_from_raw(""), MprisArtwork::None);
        assert_eq!(
            artwork_from_raw("/tmp/cover.png"),
            MprisArtwork::FilePath("/tmp/cover.png".into())
        );
        assert_eq!(
            artwork_from_raw("file:///tmp/cover.png"),
            MprisArtwork::FileUri("file:///tmp/cover.png".into())
        );
        assert_eq!(
            artwork_from_raw("https://example.com/cover.png"),
            MprisArtwork::RemoteUrl("https://example.com/cover.png".into())
        );
        assert_eq!(
            artwork_from_raw("  file:///tmp/cover.png  "),
            MprisArtwork::FileUri("file:///tmp/cover.png".into())
        );
    }

    #[test]
    fn prefers_playing_player_over_newer_paused_player() {
        let selected = select_current_player(&[
            player("spotify", MprisPlaybackStatus::Paused, 20),
            player("mpv", MprisPlaybackStatus::Playing, 10),
        ])
        .expect("current player");

        assert_eq!(selected.player_id, "mpv");
    }

    #[test]
    fn subtitle_falls_back_to_album_then_identity() {
        assert_eq!(subtitle_for("", "Promises", "Spotify"), "Promises");
        assert_eq!(subtitle_for("", "", "Spotify"), "Spotify");
    }

    #[test]
    fn panel_label_prefers_artist_and_title_then_title_then_identity() {
        assert_eq!(
            panel_label_for("Nils Frahm", "Says", "Spotify"),
            "Nils Frahm - Says"
        );
        assert_eq!(panel_label_for("", "Says", "Spotify"), "Says");
        assert_eq!(panel_label_for("", "", "Spotify"), "Spotify");
    }

    #[test]
    fn prefers_paused_player_over_newer_stopped_player() {
        let selected = select_current_player(&[
            player("vlc", MprisPlaybackStatus::Stopped, 50),
            player("spotify", MprisPlaybackStatus::Paused, 5),
        ])
        .expect("current player");

        assert_eq!(selected.player_id, "spotify");
    }

    #[test]
    fn prefers_newer_player_when_status_matches() {
        let selected = select_current_player(&[
            player("spotify", MprisPlaybackStatus::Paused, 5),
            player("mpv", MprisPlaybackStatus::Paused, 10),
        ])
        .expect("current player");

        assert_eq!(selected.player_id, "mpv");
    }

    #[test]
    fn uses_player_id_as_deterministic_tiebreaker() {
        let selected = select_current_player(&[
            player("spotify", MprisPlaybackStatus::Paused, 10),
            player("mpv", MprisPlaybackStatus::Paused, 10),
        ])
        .expect("current player");

        assert_eq!(selected.player_id, "mpv");
    }
}
