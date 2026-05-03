#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Artwork {
    #[default]
    None,
    FilePath(String),
    FileUri(String),
    RemoteUrl(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Player {
    pub player_id: String,
    pub bus_name: String,
    pub identity: String,
    pub playback_status: PlaybackStatus,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub subtitle: String,
    pub artwork: Artwork,
    pub position: Option<u64>,
    pub length: Option<u64>,
    pub progress_visible: bool,
    pub can_play_pause: bool,
    pub can_go_previous: bool,
    pub can_go_next: bool,
    pub can_raise: bool,
    pub last_active: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub current_player: Option<Player>,
    pub players: Vec<Player>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

impl Default for Health {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub health: Health,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    PlayPause { player_id: String },
    Previous { player_id: String },
    Next { player_id: String },
    Raise { player_id: String },
}

pub fn visible_players(players: &[Player]) -> Vec<Player> {
    players
        .iter()
        .filter(|player| player.playback_status != PlaybackStatus::Stopped)
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(player_id: &str, playback_status: PlaybackStatus) -> Player {
        Player {
            player_id: player_id.into(),
            playback_status,
            ..Default::default()
        }
    }

    #[test]
    fn state_defaults_to_starting_with_empty_snapshot() {
        let state = State::default();

        assert_eq!(state.health, Health::Starting);
        assert_eq!(state.snapshot, Snapshot::default());
    }

    #[test]
    fn visible_players_hide_stopped_players() {
        let players = vec![
            player("spotify", PlaybackStatus::Playing),
            player("firefox", PlaybackStatus::Paused),
            player("mpv", PlaybackStatus::Stopped),
        ];

        let ids = visible_players(&players)
            .into_iter()
            .map(|player| player.player_id)
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["spotify", "firefox"]);
    }
}
