use std::path::Path;

use crate::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};

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
        assert_eq!(playback_status_from_raw("Paused"), MprisPlaybackStatus::Paused);
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
