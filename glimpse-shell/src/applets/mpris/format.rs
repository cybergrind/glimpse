use crate::services::mpris::{PlaybackStatus, Player, State};

pub const DEFAULT_LABEL_FORMAT: &str = "{artist} - {title}";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{player}: {artist} - {title}";

pub fn label(format: &str, state: &State) -> String {
    let Some(player) = current_visible_player(state) else {
        return String::new();
    };

    let formatted = replace_placeholders(format, player).trim().to_string();
    if formatted.is_empty() {
        fallback_label(player)
    } else {
        formatted
    }
}

pub fn tooltip(format: &str, state: &State) -> String {
    let Some(player) = current_visible_player(state) else {
        return String::new();
    };

    let formatted = replace_placeholders(format, player).trim().to_string();
    if formatted.is_empty() {
        fallback_label(player)
    } else {
        formatted
    }
}

pub fn playback_status_text(status: PlaybackStatus) -> &'static str {
    match status {
        PlaybackStatus::Playing => "Playing",
        PlaybackStatus::Paused => "Paused",
        PlaybackStatus::Stopped => "Stopped",
    }
}

pub fn title(player: &Player) -> String {
    if player.title.is_empty() {
        player.identity.clone()
    } else {
        player.title.clone()
    }
}

pub fn subtitle(player: &Player) -> String {
    if !player.artist.is_empty() {
        format!("{} · {}", player.artist, player.identity)
    } else if !player.subtitle.is_empty() && player.subtitle != player.identity {
        format!("{} · {}", player.subtitle, player.identity)
    } else {
        player.identity.clone()
    }
}

pub fn duration(value_micros: u64) -> String {
    let total_seconds = value_micros / 1_000_000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

pub fn progress_fraction(position: u64, length: u64) -> f64 {
    if length == 0 {
        0.0
    } else {
        (position as f64 / length as f64).clamp(0.0, 1.0)
    }
}

fn current_visible_player(state: &State) -> Option<&Player> {
    state
        .snapshot
        .current_player
        .as_ref()
        .filter(|player| player.playback_status != PlaybackStatus::Stopped)
        .or_else(|| {
            state
                .snapshot
                .players
                .iter()
                .find(|player| player.playback_status != PlaybackStatus::Stopped)
        })
}

fn replace_placeholders(format: &str, player: &Player) -> String {
    format
        .replace("{player}", &player.identity)
        .replace("{artist}", &player.artist)
        .replace("{title}", &player.title)
        .replace("{track}", &player.title)
        .replace("{album}", &player.album)
        .replace("{state}", playback_status_text(player.playback_status))
        .replace(
            "{position}",
            &player.position.map(duration).unwrap_or_default(),
        )
        .replace(
            "{duration}",
            &player.length.map(duration).unwrap_or_default(),
        )
        .replace(
            "{remaining}",
            &remaining(player).map(duration).unwrap_or_default(),
        )
        .trim_matches([' ', '-', '—', ':'])
        .trim()
        .to_string()
}

fn remaining(player: &Player) -> Option<u64> {
    Some(player.length?.saturating_sub(player.position?))
}

fn fallback_label(player: &Player) -> String {
    if !player.artist.is_empty() && !player.title.is_empty() {
        format!("{} - {}", player.artist, player.title)
    } else if !player.title.is_empty() {
        player.title.clone()
    } else {
        player.identity.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::mpris::{State, model::Snapshot};

    fn player() -> Player {
        Player {
            player_id: "spotify".into(),
            identity: "Spotify".into(),
            playback_status: PlaybackStatus::Playing,
            title: "Says".into(),
            artist: "Nils Frahm".into(),
            album: "Spaces".into(),
            position: Some(192_000_000),
            length: Some(458_000_000),
            ..Default::default()
        }
    }

    #[test]
    fn label_renders_placeholders() {
        let player = player();
        let state = State {
            snapshot: Snapshot {
                current_player: Some(player.clone()),
                players: vec![player],
            },
            ..Default::default()
        };

        assert_eq!(label("{artist} - {title}", &state), "Nils Frahm - Says");
        assert_eq!(label("{position}/{duration}", &state), "3:12/7:38");
    }

    #[test]
    fn label_hides_empty_or_stopped_state() {
        let mut player = player();
        player.playback_status = PlaybackStatus::Stopped;
        let state = State {
            snapshot: Snapshot {
                current_player: Some(player.clone()),
                players: vec![player],
            },
            ..Default::default()
        };

        assert_eq!(label(DEFAULT_LABEL_FORMAT, &state), "");
    }
}
