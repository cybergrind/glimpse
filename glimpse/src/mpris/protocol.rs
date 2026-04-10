#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MprisPlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MprisArtwork {
    #[default]
    None,
    FilePath(String),
    FileUri(String),
    RemoteUrl(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MprisPlayer {
    pub player_id: String,
    pub bus_name: String,
    pub identity: String,
    pub playback_status: MprisPlaybackStatus,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub panel_label: String,
    pub subtitle: String,
    pub artwork: MprisArtwork,
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
pub struct MprisSnapshot {
    pub current_player: Option<MprisPlayer>,
    pub players: Vec<MprisPlayer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

impl Default for MprisServiceHealth {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MprisServiceState {
    pub health: MprisServiceHealth,
    pub snapshot: MprisSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisServiceCommand {
    PlayPause { player_id: String },
    Previous { player_id: String },
    Next { player_id: String },
    Raise { player_id: String },
    Refresh,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_state_defaults_to_starting_with_empty_snapshot() {
        let state = MprisServiceState::default();

        assert_eq!(state.health, MprisServiceHealth::Starting);
        assert_eq!(state.snapshot, MprisSnapshot::default());
    }

    #[test]
    fn artwork_default_is_none() {
        assert_eq!(MprisArtwork::default(), MprisArtwork::None);
    }
}
