#![allow(dead_code)]

use super::model::BluetoothSnapshot;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct BluetoothPromptId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothPromptKind {
    Confirm { passkey: u32 },
    RequestPin,
    RequestPasskey,
    DisplayPin { pincode: String },
    DisplayPasskey { passkey: u32, entered: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothPrompt {
    pub id: BluetoothPromptId,
    pub device_path: String,
    pub device_label: String,
    pub kind: BluetoothPromptKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothPromptReply {
    Confirm,
    Reject,
    Pin(String),
    Passkey(u32),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothActiveAction {
    SetPowered(bool),
    SetAdapterPowered {
        adapter_path: String,
        powered: bool,
    },
    SetAdapterDiscoverable {
        adapter_path: String,
        discoverable: bool,
    },
    Connect {
        address: String,
    },
    Disconnect {
        address: String,
    },
    Pair {
        address: String,
    },
    Trust {
        address: String,
        trusted: bool,
    },
    Forget {
        address: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub health: BluetoothServiceHealth,
    pub snapshot: BluetoothSnapshot,
    pub prompt: Option<BluetoothPrompt>,
    pub active_action: Option<BluetoothActiveAction>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            health: BluetoothServiceHealth::Starting,
            snapshot: BluetoothSnapshot::default(),
            prompt: None,
            active_action: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetPowered(bool),
    SetAdapterPowered {
        adapter_path: String,
        powered: bool,
    },
    SetAdapterDiscoverable {
        adapter_path: String,
        discoverable: bool,
    },
    StartDiscovery,
    StopDiscovery,
    Connect {
        address: String,
    },
    Disconnect {
        address: String,
    },
    Pair {
        address: String,
    },
    Trust {
        address: String,
        trusted: bool,
    },
    Forget {
        address: String,
    },
    PromptReply {
        id: BluetoothPromptId,
        reply: BluetoothPromptReply,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_prompt_protocol_roundtrip() {
        let prompt_id = BluetoothPromptId(7);
        let state = State {
            health: BluetoothServiceHealth::Starting,
            snapshot: BluetoothSnapshot::default(),
            prompt: Some(BluetoothPrompt {
                id: prompt_id,
                device_path: "/org/bluez/hci0/dev_AA_BB".into(),
                device_label: "Headphones".into(),
                kind: BluetoothPromptKind::RequestPin,
            }),
            active_action: None,
        };

        let cloned = state.clone();
        let reply = BluetoothPromptReply::Pin("1234".into());
        let command = Command::PromptReply {
            id: cloned.prompt.as_ref().unwrap().id,
            reply: reply.clone(),
        };

        assert_eq!(cloned.prompt.as_ref().unwrap().id.0, 7);
        assert_eq!(cloned, state);
        assert_eq!(
            command,
            Command::PromptReply {
                id: prompt_id,
                reply
            }
        );
    }
}
