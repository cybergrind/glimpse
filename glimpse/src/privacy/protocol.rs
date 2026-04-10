use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrivacySessionKind {
    Microphone,
    Camera,
    ScreenCapture,
    WindowCapture,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacySessionAction {
    StopAllScreenCapture,
    StopSession { session_id: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacySession {
    pub session_id: String,
    pub app_name: String,
    pub backend: String,
    pub started_at: Option<u64>,
    pub stoppable: bool,
    pub supported_action: Option<PrivacySessionAction>,
    pub kind: Option<PrivacySessionKind>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacyIndicatorSnapshot {
    pub mic_active: bool,
    pub camera_active: bool,
    pub screen_capture_active: bool,
    pub oldest_screen_capture_started_at: Option<u64>,
    pub session_counts: BTreeMap<PrivacySessionKind, u32>,
    pub sessions: Vec<PrivacySession>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

impl Default for PrivacyServiceHealth {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacyServiceState {
    pub health: PrivacyServiceHealth,
    pub snapshot: PrivacyIndicatorSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivacyServiceCommand {
    StopAllScreenCapture,
    StopSession { session_id: String },
    Refresh,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_state_defaults_to_empty_and_starting() {
        let state = PrivacyServiceState::default();

        assert_eq!(state.health, PrivacyServiceHealth::Starting);
        assert!(!state.snapshot.mic_active);
        assert!(state.snapshot.sessions.is_empty());
    }

    #[test]
    fn privacy_session_action_keeps_target_session_id() {
        let action = PrivacySessionAction::StopSession {
            session_id: "capture-1".into(),
        };

        assert_eq!(
            action,
            PrivacySessionAction::StopSession {
                session_id: "capture-1".into(),
            }
        );
    }
}
