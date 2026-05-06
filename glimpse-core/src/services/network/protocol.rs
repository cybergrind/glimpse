#![allow(dead_code)]

use super::NetworkSnapshot;

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
    ConnectWifi { ssid: String, path: String },
    ConnectSaved { uuid: String },
    Disconnect { uuid: String },
    Forget { uuid: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub health: NetworkServiceHealth,
    pub snapshot: NetworkSnapshot,
    pub active_action: Option<NetworkActiveAction>,
    pub scanning: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            health: NetworkServiceHealth::Starting,
            snapshot: NetworkSnapshot::default(),
            active_action: None,
            scanning: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetWifiEnabled(bool),
    StartScanning { interval_secs: u64 },
    StopScanning,
    RequestScan,
    ConnectWifi { ssid: String, path: String },
    ConnectSaved { uuid: String },
    Disconnect { uuid: String },
    Forget { uuid: String },
}

pub fn active_action_for(command: &Command) -> Option<NetworkActiveAction> {
    match command {
        Command::SetWifiEnabled(enabled) => Some(NetworkActiveAction::SetWifiEnabled(*enabled)),
        Command::ConnectWifi { ssid, path } => Some(NetworkActiveAction::ConnectWifi {
            ssid: ssid.clone(),
            path: path.clone(),
        }),
        Command::ConnectSaved { uuid } => {
            Some(NetworkActiveAction::ConnectSaved { uuid: uuid.clone() })
        }
        Command::Disconnect { uuid } => {
            Some(NetworkActiveAction::Disconnect { uuid: uuid.clone() })
        }
        Command::Forget { uuid } => Some(NetworkActiveAction::Forget { uuid: uuid.clone() }),
        Command::StartScanning { .. } | Command::StopScanning | Command::RequestScan => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_action_tracks_long_running_commands() {
        assert_eq!(
            active_action_for(&Command::SetWifiEnabled(true)),
            Some(NetworkActiveAction::SetWifiEnabled(true))
        );
        assert_eq!(
            active_action_for(&Command::ConnectWifi {
                ssid: "Office".into(),
                path: "/ap/1".into(),
            }),
            Some(NetworkActiveAction::ConnectWifi {
                ssid: "Office".into(),
                path: "/ap/1".into(),
            })
        );
        assert_eq!(
            active_action_for(&Command::Forget { uuid: "id".into() }),
            Some(NetworkActiveAction::Forget { uuid: "id".into() })
        );
    }

    #[test]
    fn scanning_commands_do_not_claim_active_action() {
        assert_eq!(
            active_action_for(&Command::StartScanning { interval_secs: 10 }),
            None
        );
        assert_eq!(active_action_for(&Command::StopScanning), None);
        assert_eq!(active_action_for(&Command::RequestScan), None);
    }
}
