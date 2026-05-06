#![allow(dead_code)]

use super::model::BluetoothSnapshot;

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
    pub active_action: Option<BluetoothActiveAction>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            health: BluetoothServiceHealth::Starting,
            snapshot: BluetoothSnapshot::default(),
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
}
