use glimpse::providers::bluetooth::BluetoothSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    Connect { address: String },
    Disconnect { address: String },
    Pair { address: String },
    Trust { address: String, trusted: bool },
    Forget { address: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothServiceState {
    pub health: BluetoothServiceHealth,
    pub snapshot: BluetoothSnapshot,
    pub prompt: Option<BluetoothPrompt>,
    pub active_action: Option<BluetoothActiveAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothServiceCommand {
    SetPowered(bool),
    StartDiscovery,
    StopDiscovery,
    Connect { address: String },
    Disconnect { address: String },
    Pair { address: String },
    Trust { address: String, trusted: bool },
    Forget { address: String },
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
        let state = BluetoothServiceState {
            health: BluetoothServiceHealth::Starting,
            snapshot: BluetoothSnapshot::default(),
            prompt: Some(BluetoothPrompt {
                id: BluetoothPromptId(7),
                device_path: "/org/bluez/hci0/dev_AA_BB".into(),
                device_label: "Headphones".into(),
                kind: BluetoothPromptKind::RequestPin,
            }),
            active_action: None,
        };

        let cloned = state.clone();

        assert_eq!(cloned.prompt.as_ref().unwrap().id.0, 7);
        assert_eq!(cloned, state);
    }
}
