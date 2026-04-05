use std::collections::HashMap;
use std::pin::Pin;

use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};
use crate::providers::dbus_props::DbusPropertyGroup;

const NAME: &str = "bluetooth";
const TOPICS: &[&str] = &[
    "bluetooth.status",
    "bluetooth.adapters",
    "bluetooth.devices",
];
const METHODS: &[&str] = &[
    "bluetooth.set_powered",
    "bluetooth.connect",
    "bluetooth.disconnect",
    "bluetooth.pair",
    "bluetooth.start_discovery",
    "bluetooth.stop_discovery",
    "bluetooth.forget",
];

#[derive(Debug, Clone, Serialize, Default)]
struct BluetoothStatus {
    powered: bool,
    discovering: bool,
    connected_count: u32,
}

#[derive(Debug, Clone, Serialize)]
struct BluetoothAdapter {
    path: String,
    name: String,
    address: String,
    powered: bool,
    discovering: bool,
}

#[derive(Debug, Clone, Serialize)]
struct BluetoothDevice {
    address: String,
    name: String,
    icon: String,
    paired: bool,
    connected: bool,
    trusted: bool,
    battery: Option<u8>,
    rssi: Option<i16>,
    adapter: String,
}

struct BluetoothProvider {
    status: BluetoothStatus,
    adapters: HashMap<String, BluetoothAdapter>,
    devices: HashMap<String, BluetoothDevice>,
}

impl Provider for BluetoothProvider {
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
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("bluetooth: starting");
            let conn = zbus::Connection::system().await?;

            self.full_scan(&conn).await;
            tracing::info!(
                adapters = self.adapters.len(),
                devices = self.devices.len(),
                powered = self.status.powered,
                "bluetooth: initial scan"
            );
            self.emit_all(&events).await;

            let om = zbus::fdo::ObjectManagerProxy::builder(&conn)
                .destination("org.bluez")?
                .path("/")?
                .build()
                .await?;
            let mut added = om.receive_interfaces_added().await?;
            let mut removed = om.receive_interfaces_removed().await?;

            let rule = "type='signal',sender='org.bluez',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'";
            conn.call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "AddMatch",
                &(rule,),
            )
            .await?;
            let mut prop_changes = zbus::MessageStream::from(&conn);

            // Debounce: coalesce rapid PropertiesChanged signals.
            let mut dirty = false;
            let debounce = tokio::time::sleep(std::time::Duration::from_millis(500));
            tokio::pin!(debounce);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req, &conn).await;
                    }
                    Some(_) = added.next() => {
                        self.full_scan(&conn).await;
                        self.emit_all(&events).await;
                    }
                    Some(_) = removed.next() => {
                        self.full_scan(&conn).await;
                        self.emit_all(&events).await;
                    }
                    Some(Ok(msg)) = prop_changes.next() => {
                        if is_bluez_properties_changed(&msg) {
                            dirty = true;
                            debounce.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(500));
                        }
                    }
                    () = &mut debounce, if dirty => {
                        dirty = false;
                        self.full_scan(&conn).await;
                        tracing::info!(
                            adapters = self.adapters.len(),
                            devices = self.devices.len(),
                            connected = self.status.connected_count,
                            "bluetooth: rescan"
                        );
                        self.emit_all(&events).await;
                    }
                }
            }
            Ok(())
        })
    }
}

use futures_util::StreamExt;

fn is_bluez_properties_changed(msg: &zbus::message::Message) -> bool {
    let header = msg.header();
    let Some(sender) = header.sender() else {
        return false;
    };
    let Some(member) = header.member() else {
        return false;
    };
    if member.as_str() != "PropertiesChanged" {
        return false;
    }
    let Some(iface) = header.interface() else {
        return false;
    };
    if iface.as_str() != "org.freedesktop.DBus.Properties" {
        return false;
    }
    // BlueZ uses a well-known name, but signals come from the unique name.
    // Check path prefix instead.
    let Some(path) = header.path() else {
        return false;
    };
    let _ = sender; // suppress unused
    path.as_str().starts_with("/org/bluez")
}

impl BluetoothProvider {
    async fn full_scan(&mut self, conn: &zbus::Connection) {
        self.adapters.clear();
        self.devices.clear();

        let om = match zbus::fdo::ObjectManagerProxy::builder(conn)
            .destination("org.bluez")
            .and_then(|b| b.path("/"))
        {
            Ok(b) => match b.build().await {
                Ok(p) => p,
                Err(_) => return,
            },
            Err(_) => return,
        };

        let objects = match om.get_managed_objects().await {
            Ok(o) => o,
            Err(_) => return,
        };

        let mut any_powered = false;
        let mut any_discovering = false;

        for (path, interfaces) in &objects {
            let path_str = path.to_string();

            if let Some(props) = interfaces.get("org.bluez.Adapter1") {
                let get_str = |key: &str| -> String {
                    props
                        .get(key)
                        .and_then(|v| String::try_from(v.clone()).ok())
                        .unwrap_or_default()
                };
                let powered = props
                    .get("Powered")
                    .and_then(|v| bool::try_from(v.clone()).ok())
                    .unwrap_or(false);
                let discovering = props
                    .get("Discovering")
                    .and_then(|v| bool::try_from(v.clone()).ok())
                    .unwrap_or(false);
                if powered {
                    any_powered = true;
                }
                if discovering {
                    any_discovering = true;
                }
                self.adapters.insert(
                    path_str.clone(),
                    BluetoothAdapter {
                        path: path_str.clone(),
                        name: get_str("Alias"),
                        address: get_str("Address"),
                        powered,
                        discovering,
                    },
                );
            }

            if let Some(props) = interfaces.get("org.bluez.Device1") {
                let get_str = |key: &str| -> String {
                    props
                        .get(key)
                        .and_then(|v| String::try_from(v.clone()).ok())
                        .unwrap_or_default()
                };
                let get_bool = |key: &str| -> bool {
                    props
                        .get(key)
                        .and_then(|v| bool::try_from(v.clone()).ok())
                        .unwrap_or(false)
                };

                let address = get_str("Address");
                let name = get_str("Alias");
                let icon = get_str("Icon");
                let paired = get_bool("Paired");
                let connected = get_bool("Connected");
                let trusted = get_bool("Trusted");

                let battery = interfaces
                    .get("org.bluez.Battery1")
                    .and_then(|bp| bp.get("Percentage"))
                    .and_then(|v| u8::try_from(v.clone()).ok());

                let rssi = props
                    .get("RSSI")
                    .and_then(|v| i16::try_from(v.clone()).ok());

                let adapter = get_str("Adapter");
                let icon_name = resolve_bt_icon(&icon, paired, connected);

                if !address.is_empty() {
                    self.devices.insert(
                        address.clone(),
                        BluetoothDevice {
                            address,
                            name: if name.is_empty() {
                                "Unknown".into()
                            } else {
                                name
                            },
                            icon: icon_name,
                            paired,
                            connected,
                            trusted,
                            battery,
                            rssi,
                            adapter,
                        },
                    );
                }
            }
        }

        self.status.powered = any_powered;
        self.status.discovering = any_discovering;
        self.status.connected_count = self.devices.values().filter(|d| d.connected).count() as u32;
    }

    async fn emit_all(&self, events: &mpsc::Sender<ProviderEvent>) {
        self.emit_status(events).await;
        self.emit_adapters(events).await;
        self.emit_devices(events).await;
    }

    async fn emit_adapters(&self, events: &mpsc::Sender<ProviderEvent>) {
        let adapters: Vec<&BluetoothAdapter> = self.adapters.values().collect();
        let _ = events
            .send(ProviderEvent {
                topic: "bluetooth.adapters".into(),
                data: serde_json::to_value(&adapters).unwrap_or_default(),
            })
            .await;
    }

    async fn emit_status(&self, events: &mpsc::Sender<ProviderEvent>) {
        let _ = events
            .send(ProviderEvent {
                topic: "bluetooth.status".into(),
                data: serde_json::to_value(&self.status).unwrap_or_default(),
            })
            .await;
    }

    async fn emit_devices(&self, events: &mpsc::Sender<ProviderEvent>) {
        let devices: Vec<&BluetoothDevice> = self.devices.values().collect();
        let _ = events
            .send(ProviderEvent {
                topic: "bluetooth.devices".into(),
                data: serde_json::to_value(&devices).unwrap_or_default(),
            })
            .await;
    }

    async fn handle_request(&mut self, req: ProviderRequest, conn: &zbus::Connection) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "bluetooth.status" => serde_json::to_value(&self.status).ok(),
                    "bluetooth.adapters" => {
                        let adapters: Vec<&BluetoothAdapter> = self.adapters.values().collect();
                        serde_json::to_value(&adapters).ok()
                    }
                    "bluetooth.devices" => {
                        let devices: Vec<&BluetoothDevice> = self.devices.values().collect();
                        serde_json::to_value(&devices).ok()
                    }
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call {
                method,
                params,
                reply,
            } => {
                let result = match method.as_str() {
                    "bluetooth.set_powered" => {
                        let Some(powered) = params["powered"].as_bool() else {
                            let _ = reply.send(Err(anyhow::anyhow!("missing 'powered' (bool)")));
                            return;
                        };
                        tracing::info!("changing bluetooth power state to {}", powered);
                        self.adapter_set(conn, &params, "Powered", powered).await
                    }
                    "bluetooth.connect" => {
                        tracing::info!(
                            "connecting to {}",
                            params["address"].as_str().unwrap_or("?")
                        );
                        self.device_call(conn, &params, "Connect").await
                    }
                    "bluetooth.disconnect" => {
                        tracing::info!(
                            "disconnecting from {}",
                            params["address"].as_str().unwrap_or("?")
                        );
                        self.device_call(conn, &params, "Disconnect").await
                    }
                    "bluetooth.pair" => {
                        tracing::info!(
                            "pairing with {}",
                            params["address"].as_str().unwrap_or("?")
                        );
                        self.device_call(conn, &params, "Pair").await
                    }
                    "bluetooth.forget" => {
                        let address = params["address"].as_str().unwrap_or("");
                        tracing::info!("forgetting device {}", address);
                        let Some(dev) = self.devices.get(address) else {
                            let _ = reply.send(Err(anyhow::anyhow!("unknown device: {address}")));
                            return;
                        };
                        let adapter_path = dev.adapter.clone();
                        let dev_path = device_path(&adapter_path, address);
                        let adapter = DbusPropertyGroup::new(
                            conn,
                            "org.bluez",
                            &adapter_path,
                            "org.bluez.Adapter1",
                        )
                        .await;
                        let result = match adapter {
                            Ok(a) => match zbus::zvariant::ObjectPath::try_from(dev_path) {
                                Ok(obj_path) => a
                                    .call::<_, ()>("RemoveDevice", &(obj_path,))
                                    .await
                                    .map(|()| json!(null))
                                    .map_err(|e| anyhow::anyhow!("{e}")),
                                Err(e) => Err(anyhow::anyhow!("{e}")),
                            },
                            Err(e) => Err(anyhow::anyhow!("{e}")),
                        };
                        let _ = reply.send(result);
                        return;
                    }
                    "bluetooth.start_discovery" => {
                        tracing::info!("starting bluetooth discovery");
                        self.adapter_call(conn, &params, "StartDiscovery").await
                    }
                    "bluetooth.stop_discovery" => {
                        tracing::info!("stopping bluetooth discovery");
                        self.adapter_call(conn, &params, "StopDiscovery").await
                    }
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                if let Err(ref e) = result {
                    tracing::warn!(method = %method, error = %e, "bluetooth: call failed");
                }
                let _ = reply.send(result);
            }
        }
    }

    fn resolve_adapter_paths(&self, params: &serde_json::Value) -> anyhow::Result<Vec<String>> {
        if self.adapters.is_empty() {
            return Err(anyhow::anyhow!("no bluetooth adapters found"));
        }
        if let Some(path) = params["adapter"].as_str() {
            if self.adapters.contains_key(path) {
                return Ok(vec![path.to_owned()]);
            }
            return Err(anyhow::anyhow!("unknown adapter: {path}"));
        }
        Ok(self.adapters.keys().cloned().collect())
    }

    async fn adapter_set<
        T: Into<zbus::zvariant::Value<'static>> + Send + Sync + Clone + 'static,
    >(
        &self,
        conn: &zbus::Connection,
        params: &serde_json::Value,
        prop: &str,
        value: T,
    ) -> anyhow::Result<serde_json::Value> {
        let paths = self.resolve_adapter_paths(params)?;
        let mut last_err = None;
        for path in &paths {
            match DbusPropertyGroup::new(conn, "org.bluez", path, "org.bluez.Adapter1").await {
                Ok(a) => {
                    if let Err(e) = a.set(prop, value.clone()).await {
                        last_err = Some(format!("{e}"));
                    }
                }
                Err(e) => {
                    last_err = Some(format!("{e}"));
                }
            }
        }
        match last_err {
            Some(e) => Err(anyhow::anyhow!("{e}")),
            None => Ok(json!(null)),
        }
    }

    async fn adapter_call(
        &self,
        conn: &zbus::Connection,
        params: &serde_json::Value,
        method: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let paths = self.resolve_adapter_paths(params)?;
        let mut last_err = None;
        for path in &paths {
            match DbusPropertyGroup::new(conn, "org.bluez", path, "org.bluez.Adapter1").await {
                Ok(a) => {
                    if let Err(e) = a.call_void(method, &()).await {
                        last_err = Some(format!("{e}"));
                    }
                }
                Err(e) => {
                    last_err = Some(format!("{e}"));
                }
            }
        }
        match last_err {
            Some(e) => Err(anyhow::anyhow!("{e}")),
            None => Ok(json!(null)),
        }
    }

    async fn device_call(
        &self,
        conn: &zbus::Connection,
        params: &serde_json::Value,
        method: &str,
    ) -> anyhow::Result<serde_json::Value> {
        let address = params["address"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'address'"))?;
        let dev = self
            .devices
            .get(address)
            .ok_or_else(|| anyhow::anyhow!("unknown device: {address}"))?;
        let path = device_path(&dev.adapter, address);
        let proxy = DbusPropertyGroup::new(conn, "org.bluez", &path, "org.bluez.Device1")
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        proxy
            .call::<_, ()>(method, &())
            .await
            .map(|()| json!(null))
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

fn device_path(adapter_path: &str, address: &str) -> String {
    format!("{}/dev_{}", adapter_path, address.replace(':', "_"))
}

fn resolve_bt_icon(icon_hint: &str, _paired: bool, connected: bool) -> String {
    let base = match icon_hint {
        "audio-headphones" => "audio-headphones-symbolic",
        "audio-headset" => "audio-headset-symbolic",
        "audio-speakers" | "audio-card" => "audio-speakers-symbolic",
        "input-keyboard" => "input-keyboard-symbolic",
        "input-mouse" => "input-mouse-symbolic",
        "input-tablet" => "input-tablet-symbolic",
        "input-gaming" => "input-gaming-symbolic",
        "phone" => "phone-symbolic",
        "computer" => "computer-symbolic",
        "video-display" => "video-display-symbolic",
        _ if icon_hint.contains("headphone") => "audio-headphones-symbolic",
        _ if icon_hint.contains("headset") => "audio-headset-symbolic",
        _ if icon_hint.contains("keyboard") => "input-keyboard-symbolic",
        _ if icon_hint.contains("mouse") => "input-mouse-symbolic",
        _ if icon_hint.contains("phone") => "phone-symbolic",
        _ => {
            if connected {
                "bluetooth-active-symbolic"
            } else {
                "bluetooth-symbolic"
            }
        }
    };
    base.to_owned()
}

pub struct BluetoothProviderFactory;

impl ProviderFactory for BluetoothProviderFactory {
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
        Box::new(BluetoothProvider {
            status: BluetoothStatus::default(),
            adapters: HashMap::new(),
            devices: HashMap::new(),
        })
    }
}
