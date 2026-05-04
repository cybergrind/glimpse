#![allow(dead_code)]

use std::time::Duration;

use anyhow::{Context, bail};
use futures_util::{StreamExt, future};
use tokio::{sync::mpsc, time::Instant};
use tokio_util::sync::CancellationToken;
use zbus::{MatchRule, MessageStream, message::Type, zvariant::ObjectPath};

use crate::dbus::bluez::{Adapter1Proxy, Battery1Proxy, Device1Proxy};

use super::model::{
    BluetoothAdapter, BluetoothChangeReason, BluetoothDevice, BluetoothDeviceType,
    BluetoothSnapshot, BluezEvent, device_display_name,
};

const LISTENER_DEBOUNCE: Duration = Duration::from_millis(300);

#[derive(Clone)]
pub struct BluezClient {
    conn: zbus::Connection,
}

impl BluezClient {
    pub fn new(conn: zbus::Connection) -> Self {
        Self { conn }
    }

    pub async fn scan(&self) -> anyhow::Result<BluetoothSnapshot> {
        let om = self.object_manager().await?;
        let objects = om
            .get_managed_objects()
            .await
            .context("failed to read BlueZ managed objects")?;

        let mut adapters = Vec::new();
        let mut devices = Vec::new();

        for (path, interfaces) in &objects {
            let path_str = path.to_string();

            if interfaces.contains_key("org.bluez.Adapter1") {
                let adapter = self
                    .read_adapter(&path_str)
                    .await
                    .with_context(|| format!("failed to read adapter {path_str}"))?;
                adapters.push(adapter);
            }

            if interfaces.contains_key("org.bluez.Device1") {
                let device = self
                    .read_device(&path_str, interfaces.contains_key("org.bluez.Battery1"))
                    .await
                    .with_context(|| format!("failed to read device {path_str}"))?;

                if device.address.is_empty() {
                    tracing::debug!(path = %path_str, "bluetooth: skipping transient device without address");
                    continue;
                }

                devices.push(device);
            }
        }

        let snapshot = BluetoothSnapshot::new(adapters, devices);
        tracing::debug!(
            adapters = snapshot.adapters.len(),
            devices = snapshot.devices.len(),
            powered = snapshot.status.powered,
            discovering = snapshot.status.discovering,
            connected = snapshot.status.connected_count,
            "bluetooth: scan complete"
        );

        if snapshot.adapters.is_empty() {
            tracing::info!("bluetooth: no adapters found");
        } else if !snapshot.status.powered {
            tracing::info!(
                adapters = snapshot.adapters.len(),
                "bluetooth: adapters present but all are powered off"
            );
        }

        Ok(snapshot)
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<BluezEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::info!("bluetooth: listener started");

        let om = self.object_manager().await?;
        let mut added = om.receive_interfaces_added().await?;
        let mut removed = om.receive_interfaces_removed().await?;
        let mut properties = self.properties_changed_stream().await?;

        let mut pending_reason: Option<BluetoothChangeReason> = None;
        let mut debounce_deadline: Option<Instant> = None;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("bluetooth: listener stopping");
                    break;
                }
                signal = added.next() => {
                    match signal {
                        Some(_) => {
                            tracing::debug!("bluetooth: interfaces added signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, BluetoothChangeReason::InterfacesAdded));
                            debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                        }
                        None => {
                            tracing::warn!("bluetooth: interfaces-added stream ended");
                            break;
                        }
                    }
                }
                signal = removed.next() => {
                    match signal {
                        Some(_) => {
                            tracing::debug!("bluetooth: interfaces removed signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, BluetoothChangeReason::InterfacesRemoved));
                            debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                        }
                        None => {
                            tracing::warn!("bluetooth: interfaces-removed stream ended");
                            break;
                        }
                    }
                }
                signal = properties.next() => {
                    match signal {
                        Some(Ok(message)) => {
                            if !is_bluez_properties_changed(&message) {
                                continue;
                            }
                            tracing::debug!("bluetooth: properties changed signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, BluetoothChangeReason::PropertiesChanged));
                            debounce_deadline = Some(Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Err(error)) => {
                            tracing::warn!(error = %error, "bluetooth: properties stream error");
                        }
                        None => {
                            tracing::warn!("bluetooth: properties stream ended");
                            break;
                        }
                    }
                }
                _ = async {
                    match debounce_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => future::pending::<()>().await,
                    }
                }, if debounce_deadline.is_some() => {
                    let reason = pending_reason.take().unwrap_or(BluetoothChangeReason::Mixed);
                    debounce_deadline = None;
                    tracing::debug!(reason = %reason, "bluetooth: change event emitted");
                    if events.send(BluezEvent::Changed { reason }).await.is_err() {
                        tracing::info!("bluetooth: listener receiver dropped");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn set_powered(&self, powered: bool) -> anyhow::Result<()> {
        let adapter_paths = self.adapter_paths().await?;
        if adapter_paths.is_empty() {
            bail!("no bluetooth adapters found");
        }

        tracing::info!(
            powered,
            adapters = adapter_paths.len(),
            "bluetooth: set power requested"
        );

        for path in adapter_paths {
            let proxy = self.adapter_proxy(&path).await?;
            proxy
                .set_powered(powered)
                .await
                .with_context(|| format!("failed to set adapter power on {path}"))?;
            tracing::debug!(path = %path, powered, "bluetooth: adapter power updated");
        }

        tracing::info!(powered, "bluetooth: set power succeeded");
        Ok(())
    }

    pub async fn set_adapter_powered(
        &self,
        adapter_path: &str,
        powered: bool,
    ) -> anyhow::Result<()> {
        tracing::info!(path = %adapter_path, powered, "bluetooth: set adapter power requested");
        let proxy = self.adapter_proxy(adapter_path).await?;
        proxy
            .set_powered(powered)
            .await
            .with_context(|| format!("failed to set adapter power on {adapter_path}"))?;
        tracing::info!(path = %adapter_path, powered, "bluetooth: set adapter power succeeded");
        Ok(())
    }

    pub async fn set_adapter_discoverable(
        &self,
        adapter_path: &str,
        discoverable: bool,
    ) -> anyhow::Result<()> {
        tracing::info!(
            path = %adapter_path,
            discoverable,
            "bluetooth: set adapter discoverable requested"
        );
        let proxy = self.adapter_proxy(adapter_path).await?;
        proxy
            .set_discoverable(discoverable)
            .await
            .with_context(|| format!("failed to set adapter discoverable on {adapter_path}"))?;
        tracing::info!(
            path = %adapter_path,
            discoverable,
            "bluetooth: set adapter discoverable succeeded"
        );
        Ok(())
    }

    pub async fn connect(&self, address: &str) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            "bluetooth: connect requested"
        );
        let proxy = self.device_proxy(&device.path).await?;
        proxy
            .connect()
            .await
            .with_context(|| format!("failed to connect {}", device.address))?;
        tracing::info!(address = %device.address, name = %device.name, "bluetooth: connect succeeded");
        Ok(())
    }

    pub async fn disconnect(&self, address: &str) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            "bluetooth: disconnect requested"
        );
        let proxy = self.device_proxy(&device.path).await?;
        proxy
            .disconnect()
            .await
            .with_context(|| format!("failed to disconnect {}", device.address))?;
        tracing::info!(address = %device.address, name = %device.name, "bluetooth: disconnect succeeded");
        Ok(())
    }

    pub async fn pair(&self, address: &str) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            "bluetooth: pair requested"
        );
        let proxy = self.device_proxy(&device.path).await?;
        proxy
            .pair()
            .await
            .with_context(|| format!("failed to pair {}", device.address))?;
        tracing::debug!(
            address = %device.address,
            name = %device.name,
            "bluetooth: trusting paired device"
        );
        proxy
            .set_trusted(true)
            .await
            .with_context(|| format!("failed to trust paired device {}", device.address))?;
        tracing::debug!(
            address = %device.address,
            name = %device.name,
            "bluetooth: connecting paired device"
        );
        if proxy.connected().await.unwrap_or(false) {
            tracing::debug!(
                address = %device.address,
                name = %device.name,
                "bluetooth: paired device already connected"
            );
        } else {
            proxy
                .connect()
                .await
                .with_context(|| format!("failed to connect paired device {}", device.address))?;
        }
        tracing::info!(
            address = %device.address,
            name = %device.name,
            "bluetooth: pair, trust, and connect succeeded"
        );
        Ok(())
    }

    pub async fn trust(&self, address: &str, trusted: bool) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            trusted,
            action = trust_action(trusted),
            "bluetooth: trust requested"
        );
        let proxy = self.device_proxy(&device.path).await?;
        proxy
            .set_trusted(trusted)
            .await
            .with_context(|| format!("failed to set trust for {}", device.address))?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            trusted,
            action = trust_action(trusted),
            status = trust_status(trusted),
            "bluetooth: trust succeeded"
        );
        Ok(())
    }

    pub async fn forget(&self, address: &str) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            adapter = %device.adapter_path,
            "bluetooth: forget requested"
        );
        let proxy = self.adapter_proxy(&device.adapter_path).await?;
        let device_path = ObjectPath::try_from(device.path.as_str())
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        proxy
            .remove_device(device_path)
            .await
            .with_context(|| format!("failed to forget {}", device.address))?;
        tracing::info!(address = %device.address, name = %device.name, "bluetooth: forget succeeded");
        Ok(())
    }

    pub async fn start_discovery(&self) -> anyhow::Result<()> {
        let adapter_paths = self.adapter_paths().await?;
        if adapter_paths.is_empty() {
            tracing::info!("bluetooth: start discovery skipped; no adapters found");
            return Ok(());
        }

        tracing::info!(
            adapters = adapter_paths.len(),
            "bluetooth: start discovery requested"
        );

        for path in adapter_paths {
            let proxy = self.adapter_proxy(&path).await?;
            proxy
                .start_discovery()
                .await
                .with_context(|| format!("failed to start discovery on {path}"))?;
            tracing::debug!(path = %path, "bluetooth: discovery started on adapter");
        }

        Ok(())
    }

    pub async fn stop_discovery(&self) -> anyhow::Result<()> {
        let adapter_paths = self.adapter_paths().await?;
        if adapter_paths.is_empty() {
            tracing::debug!("bluetooth: stop discovery skipped; no adapters found");
            return Ok(());
        }

        for path in &adapter_paths {
            let proxy = self.adapter_proxy(path).await?;
            proxy
                .stop_discovery()
                .await
                .with_context(|| format!("failed to stop discovery on {path}"))?;
            tracing::debug!(path = %path, "bluetooth: discovery stopped on adapter");
        }

        tracing::info!("bluetooth: discovery stopped");
        Ok(())
    }

    async fn adapter_paths(&self) -> anyhow::Result<Vec<String>> {
        let om = self.object_manager().await?;
        let objects = om
            .get_managed_objects()
            .await
            .context("failed to read BlueZ managed objects for adapters")?;
        let mut paths = objects
            .iter()
            .filter(|(_, interfaces)| interfaces.contains_key("org.bluez.Adapter1"))
            .map(|(path, _)| path.to_string())
            .collect::<Vec<_>>();
        paths.sort();
        Ok(paths)
    }

    async fn object_manager(&self) -> anyhow::Result<zbus::fdo::ObjectManagerProxy<'_>> {
        zbus::fdo::ObjectManagerProxy::builder(&self.conn)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn adapter_proxy<'a>(&self, path: &'a str) -> anyhow::Result<Adapter1Proxy<'a>> {
        Adapter1Proxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn device_proxy<'a>(&self, path: &'a str) -> anyhow::Result<Device1Proxy<'a>> {
        Device1Proxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn read_adapter(&self, path: &str) -> anyhow::Result<BluetoothAdapter> {
        let proxy = self.adapter_proxy(path).await?;
        Ok(BluetoothAdapter {
            path: path.to_owned(),
            name: {
                let alias = proxy.alias().await.unwrap_or_default();
                if alias.is_empty() {
                    proxy.name().await.unwrap_or_default()
                } else {
                    alias
                }
            },
            address: proxy.address().await.unwrap_or_default(),
            powered: proxy.powered().await.unwrap_or(false),
            discovering: proxy.discovering().await.unwrap_or(false),
            discoverable: proxy.discoverable().await.unwrap_or(false),
            pairable: proxy.pairable().await.unwrap_or(false),
            address_type: proxy.address_type().await.unwrap_or_default(),
            class: proxy.class().await.unwrap_or_default(),
            discoverable_timeout: proxy.discoverable_timeout().await.unwrap_or_default(),
            pairable_timeout: proxy.pairable_timeout().await.unwrap_or_default(),
            modalias: proxy.modalias().await.unwrap_or_default(),
            roles: proxy.roles().await.unwrap_or_default(),
            uuids: proxy.uuids().await.unwrap_or_default(),
        })
    }

    async fn read_device(&self, path: &str, has_battery: bool) -> anyhow::Result<BluetoothDevice> {
        let proxy = self.device_proxy(path).await?;
        let address = proxy.address().await.unwrap_or_default();
        let alias = proxy.alias().await.unwrap_or_default();
        let icon = proxy.icon().await.unwrap_or_default();
        let paired = proxy.paired().await.unwrap_or(false);
        let connected = proxy.connected().await.unwrap_or(false);
        let trusted = proxy.trusted().await.unwrap_or(false);
        let rssi = proxy.rssi().await.ok();
        let class = proxy.class().await.unwrap_or(0);
        let appearance = proxy.appearance().await.unwrap_or(0);
        let adapter = proxy
            .adapter()
            .await
            .map(|path| path.to_string())
            .unwrap_or_default();
        let battery = if has_battery {
            self.read_battery_percentage(path).await
        } else {
            None
        };

        Ok(BluetoothDevice {
            path: path.to_owned(),
            name: device_display_name(&alias, &address),
            address,
            alias,
            device_type: BluetoothDeviceType::from_hints(appearance, class, &icon),
            paired,
            connected,
            trusted,
            battery,
            rssi,
            class,
            appearance,
            adapter,
        })
    }

    async fn read_battery_percentage(&self, path: &str) -> Option<u8> {
        let proxy = Battery1Proxy::builder(&self.conn)
            .path(path)
            .ok()?
            .build()
            .await
            .ok()?;
        proxy.percentage().await.ok()
    }

    async fn properties_changed_stream(&self) -> anyhow::Result<MessageStream> {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.bluez")?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .build();

        MessageStream::for_match_rule(rule, &self.conn, None)
            .await
            .map_err(Into::into)
    }

    async fn resolve_device(&self, address: &str) -> anyhow::Result<ResolvedDevice> {
        let om = self.object_manager().await?;
        let objects = om
            .get_managed_objects()
            .await
            .context("failed to read BlueZ managed objects for device lookup")?;

        for (path, interfaces) in &objects {
            let Some(props) = interfaces.get("org.bluez.Device1") else {
                continue;
            };

            let current_address = props
                .get("Address")
                .and_then(|value| String::try_from(value.clone()).ok())
                .unwrap_or_default();
            if current_address != address {
                continue;
            }

            let name = props
                .get("Alias")
                .and_then(|value| String::try_from(value.clone()).ok())
                .unwrap_or_default();
            let adapter_path = props
                .get("Adapter")
                .and_then(|value| {
                    zbus::zvariant::ObjectPath::try_from(value.clone())
                        .map(|path| path.to_string())
                        .ok()
                })
                .unwrap_or_default();

            return Ok(ResolvedDevice {
                path: path.to_string(),
                adapter_path,
                address: current_address,
                name: device_display_name(&name, address),
            });
        }

        bail!("unknown bluetooth device: {address}")
    }
}

struct ResolvedDevice {
    path: String,
    adapter_path: String,
    address: String,
    name: String,
}

fn trust_action(trusted: bool) -> &'static str {
    if trusted { "trust" } else { "untrust" }
}

fn trust_status(trusted: bool) -> &'static str {
    if trusted { "trusted" } else { "untrusted" }
}

fn merge_change_reason(
    current: Option<BluetoothChangeReason>,
    next: BluetoothChangeReason,
) -> BluetoothChangeReason {
    match current {
        None => next,
        Some(current) if current == next => current,
        Some(_) => BluetoothChangeReason::Mixed,
    }
}

fn is_bluez_properties_changed(message: &zbus::message::Message) -> bool {
    let header = message.header();

    let Some(member) = header.member() else {
        return false;
    };
    if member.as_str() != "PropertiesChanged" {
        return false;
    }

    let Some(interface) = header.interface() else {
        return false;
    };
    if interface.as_str() != "org.freedesktop.DBus.Properties" {
        return false;
    }

    let Some(path) = header.path() else {
        return false;
    };
    path.as_str().starts_with("/org/bluez")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trust_helpers_distinguish_enable_and_disable_semantics() {
        assert_eq!(trust_action(true), "trust");
        assert_eq!(trust_action(false), "untrust");
        assert_eq!(trust_status(true), "trusted");
        assert_eq!(trust_status(false), "untrusted");
    }

    #[test]
    fn merge_change_reason_keeps_same_reason_and_marks_mixed_bursts() {
        assert_eq!(
            merge_change_reason(None, BluetoothChangeReason::InterfacesAdded),
            BluetoothChangeReason::InterfacesAdded
        );
        assert_eq!(
            merge_change_reason(
                Some(BluetoothChangeReason::InterfacesAdded),
                BluetoothChangeReason::InterfacesAdded,
            ),
            BluetoothChangeReason::InterfacesAdded
        );
        assert_eq!(
            merge_change_reason(
                Some(BluetoothChangeReason::InterfacesAdded),
                BluetoothChangeReason::PropertiesChanged,
            ),
            BluetoothChangeReason::Mixed
        );
    }
}
