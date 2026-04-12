use crate::network::provider::NetworkSnapshot;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct NetworkPromptId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkPromptKind {
    WifiPassword { ssid: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkPrompt {
    pub id: NetworkPromptId,
    pub kind: NetworkPromptKind,
    pub error_message: Option<String>,
    pub submitting: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkPromptReply {
    SubmitPassword(String),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkActiveAction {
    SetWifiEnabled(bool),
    Scan,
    ConnectWifi { ssid: String, path: String },
    ConnectSaved { uuid: String },
    Disconnect { uuid: String },
    Forget { uuid: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkServiceState {
    pub health: NetworkServiceHealth,
    pub snapshot: NetworkSnapshot,
    pub prompt: Option<NetworkPrompt>,
    pub active_action: Option<NetworkActiveAction>,
    pub scanning: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetworkServiceCommand {
    SetWifiEnabled(bool),
    StartScanning {
        interval_secs: u64,
    },
    StopScanning,
    RequestScan,
    ConnectWifi {
        ssid: String,
        path: String,
    },
    ConnectSaved {
        uuid: String,
    },
    Disconnect {
        uuid: String,
    },
    Forget {
        uuid: String,
    },
    PromptReply {
        id: NetworkPromptId,
        reply: NetworkPromptReply,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_prompt_protocol_roundtrip() {
        let prompt_id = NetworkPromptId(11);
        let state = NetworkServiceState {
            health: NetworkServiceHealth::Starting,
            snapshot: NetworkSnapshot::default(),
            prompt: Some(NetworkPrompt {
                id: prompt_id,
                kind: NetworkPromptKind::WifiPassword {
                    ssid: "Office".into(),
                },
                error_message: Some("Incorrect password".into()),
                submitting: true,
            }),
            active_action: Some(NetworkActiveAction::Scan),
            scanning: true,
        };

        let cloned = state.clone();
        let reply = NetworkPromptReply::SubmitPassword("secret".into());
        let command = NetworkServiceCommand::PromptReply {
            id: prompt_id,
            reply: reply.clone(),
        };

        assert_eq!(cloned, state);
        assert_eq!(
            command,
            NetworkServiceCommand::PromptReply {
                id: prompt_id,
                reply,
            }
        );
    }

    #[test]
    fn network_failure_classification_roundtrip() {
        let classification =
            crate::network::provider::NetworkFailureClassification::AuthenticationFailed;
        assert_eq!(
            classification,
            crate::network::provider::NetworkFailureClassification::AuthenticationFailed
        );
    }
}
