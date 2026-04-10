use crate::mpris::protocol::{MprisPlaybackStatus, MprisPlayer};

pub fn subtitle_for(artist: &str, _title: &str, album: &str, identity: &str) -> String {
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
            is_current: false,
        }
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
        assert_eq!(subtitle_for("", "", "Promises", "Spotify"), "Promises");
        assert_eq!(subtitle_for("", "", "", "Spotify"), "Spotify");
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
