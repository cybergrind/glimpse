#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PrivacyServiceHealth {
    #[default]
    Starting,
    Ready,
    Degraded { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrivacySnapshot {
    pub camera_blocked: bool,
    pub microphone_blocked: bool,
    pub location_blocked: bool,
    pub screen_share_blocked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrivacyServiceState {
    pub health: PrivacyServiceHealth,
    pub snapshot: PrivacySnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_service_state_defaults_to_starting_with_unblocked_snapshot() {
        let state = PrivacyServiceState::default();

        assert_eq!(state.health, PrivacyServiceHealth::Starting);
        assert_eq!(state.snapshot, PrivacySnapshot::default());
    }

    #[test]
    fn privacy_snapshot_defaults_to_all_false() {
        let snapshot = PrivacySnapshot::default();

        assert!(!snapshot.camera_blocked);
        assert!(!snapshot.microphone_blocked);
        assert!(!snapshot.location_blocked);
        assert!(!snapshot.screen_share_blocked);
    }
}
