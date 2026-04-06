use std::pin::Pin;

use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "network";
const TOPICS: &[&str] = &[
    "network.status",
    "network.wifi",
    "network.connections",
    "network.devices",
    "network.saved_vpns",
];
const METHODS: &[&str] = &[
    "network.set_wifi_enabled",
    "network.set_enabled",
    "network.wifi_scan",
    "network.connect",
    "network.connect_uuid",
    "network.disconnect",
    "network.forget",
];

#[derive(Debug, Clone, Serialize, Default)]
struct NetworkStatus {
    connectivity: String,
    enabled: bool,
    wifi_enabled: bool,
    wifi_hw_enabled: bool,
    primary_connection: String,
    primary_type: String,
    metered: bool,
    speed: u32,
    icon: String,
}

#[derive(Debug, Clone, Serialize)]
struct WifiAccessPoint {
    ssid: String,
    strength: u8,
    frequency: u32,
    security: String,
    connected: bool,
    saved: bool,
    uuid: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct NetworkConnection {
    id: String,
    uuid: String,
    connection_type: String,
    device: String,
    state: String,
    vpn: bool,
    ip4_address: Option<String>,
    gateway: Option<String>,
    dns: Vec<String>,
    speed: u32,
}

#[derive(Debug, Clone, Serialize)]
struct NetworkDevice {
    interface: String,
    device_type: String,
    state: String,
    speed: u32,
    carrier: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
struct SavedVpn {
    id: String,
    uuid: String,
    connection_type: String,
    active: bool,
    state: Option<String>,
}

struct NetworkProvider {
    status: NetworkStatus,
    access_points: Vec<WifiAccessPoint>,
    connections: Vec<NetworkConnection>,
    devices: Vec<NetworkDevice>,
    saved_vpns: Vec<SavedVpn>,
}

impl Provider for NetworkProvider {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }

    fn run(
        &mut self,
        _events: mpsc::Sender<ProviderEvent>,
        _requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("network: starting");
            cancel.cancelled().await;
            Ok(())
        })
    }
}

pub struct NetworkProviderFactory;

impl ProviderFactory for NetworkProviderFactory {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(NetworkProvider {
            status: NetworkStatus::default(),
            access_points: Vec::new(),
            connections: Vec::new(),
            devices: Vec::new(),
            saved_vpns: Vec::new(),
        })
    }
}
