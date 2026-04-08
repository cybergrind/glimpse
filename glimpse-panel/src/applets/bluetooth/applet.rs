use std::sync::Arc;

use glimpse::providers::bluetooth::{
    BluetoothChangeReason, BluetoothDevice, BluetoothProvider, BluetoothProviderEvent,
    BluetoothSnapshot,
};
use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::config::BluetoothConfig;
use super::popover::{BluetoothPopover, BluetoothPopoverInit, BluetoothPopoverInput, BtDevice};

pub struct Bluetooth {
    icon_name: String,
    tooltip: String,
    popover: Controller<BluetoothPopover>,
}

pub struct BluetoothInit {
    pub config: BluetoothConfig,
    pub conn: zbus::Connection,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum BluetoothMsg {
    StatusUpdate {
        powered: bool,
        discovering: bool,
        connected_count: u32,
    },
    DevicesUpdate(Vec<BtDevice>),
    ListenerChanged(BluetoothChangeReason),
    TogglePopover,
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Bluetooth {
    type Init = BluetoothInit;
    type Input = BluetoothMsg;
    type Output = ();
    type CommandOutput = BluetoothMsg;

    view! {
        gtk::Box {
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "bluetooth",
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(BluetoothMsg::TogglePopover);
                }
            },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 16,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let conn = init.conn;
        let popover = BluetoothPopover::builder()
            .launch(BluetoothPopoverInit {
                parent: root.clone(),
                client: init.client.clone(),
                settings_command: init.config.settings_command,
                scan_interval: init.config.scan_interval,
            })
            .detach();

        let model = Bluetooth {
            icon_name: "bluetooth-active-symbolic".into(),
            tooltip: "Bluetooth".into(),
            popover,
        };

        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("bluetooth applet: starting provider listener");
                    let cancel = CancellationToken::new();
                    let provider = BluetoothProvider::new(conn.clone());

                    if let Err(error) = refresh_snapshot(&provider, &out).await {
                        tracing::warn!(error = %error, "bluetooth applet: initial scan failed");
                    }

                    let (event_tx, mut event_rx) = mpsc::channel::<BluetoothProviderEvent>(16);
                    tokio::spawn({
                        let cancel = cancel.clone();
                        let provider = BluetoothProvider::new(conn);
                        async move {
                            if let Err(error) = provider.listen(event_tx, cancel).await {
                                tracing::error!(error = %error, "bluetooth applet: listener failed");
                            }
                        }
                    });

                    while let Some(event) = event_rx.recv().await {
                        match event {
                            BluetoothProviderEvent::Changed { reason } => {
                                let _ = out.send(BluetoothMsg::ListenerChanged(reason));
                                if let Err(error) = refresh_snapshot(&provider, &out).await {
                                    tracing::warn!(
                                        reason = %reason,
                                        error = %error,
                                        "bluetooth applet: refresh failed after listener event"
                                    );
                                }
                            }
                        }
                    }

                    tracing::warn!("bluetooth applet: listener channel closed");
                    cancel.cancel();
                    let _ = out.send(BluetoothMsg::Unavailable);
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            BluetoothMsg::StatusUpdate {
                powered,
                discovering,
                connected_count,
            } => {
                tracing::info!(
                    powered,
                    discovering,
                    connected_count,
                    "bluetooth applet: status update"
                );
                self.icon_name = if !powered {
                    "bluetooth-disabled-symbolic"
                } else {
                    "bluetooth-active-symbolic"
                }
                .into();

                self.tooltip = if !powered {
                    "Bluetooth off".into()
                } else if connected_count > 0 {
                    format!(
                        "{connected_count} device{} connected",
                        if connected_count > 1 { "s" } else { "" }
                    )
                } else {
                    "Bluetooth".into()
                };

                self.popover.emit(BluetoothPopoverInput::UpdateStatus {
                    powered,
                    discovering,
                });
            }
            BluetoothMsg::DevicesUpdate(devices) => {
                self.popover
                    .emit(BluetoothPopoverInput::UpdateDevices(devices));
            }
            BluetoothMsg::ListenerChanged(reason) => {
                tracing::info!(reason = %reason, "bluetooth applet: listener event");
            }
            BluetoothMsg::TogglePopover => {
                self.popover.emit(BluetoothPopoverInput::Toggle);
            }
            BluetoothMsg::Unavailable => {
                tracing::warn!("bluetooth applet: daemon unavailable");
            }
        }
    }
}

async fn refresh_snapshot(
    provider: &BluetoothProvider,
    out: &relm4::Sender<BluetoothMsg>,
) -> Result<(), String> {
    let snapshot = provider.scan().await.map_err(|error| error.to_string())?;
    let _ = out.send(BluetoothMsg::StatusUpdate {
        powered: snapshot.status.powered,
        discovering: snapshot.status.discovering,
        connected_count: snapshot.status.connected_count,
    });
    let _ = out.send(BluetoothMsg::DevicesUpdate(popover_devices(snapshot)));
    Ok(())
}

fn popover_devices(snapshot: BluetoothSnapshot) -> Vec<BtDevice> {
    snapshot.devices.into_iter().map(popover_device).collect()
}

fn popover_device(device: BluetoothDevice) -> BtDevice {
    BtDevice {
        address: device.address,
        name: device.name,
        icon: device.device_type.icon(device.connected).to_owned(),
        device_type: device.device_type.label().to_owned(),
        paired: device.paired,
        trusted: device.trusted,
        connected: device.connected,
        battery: device.battery,
        rssi: device.rssi,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popover_device_uses_provider_type_metadata() {
        let device = BluetoothDevice {
            address: "AA:BB:CC:DD:EE:FF".into(),
            name: "Headphones".into(),
            device_type: glimpse::providers::bluetooth::BluetoothDeviceType::Headphones,
            paired: true,
            connected: true,
            trusted: true,
            battery: Some(75),
            rssi: Some(-30),
            adapter: "/org/bluez/hci0".into(),
        };

        let mapped = popover_device(device);

        assert_eq!(mapped.icon, "audio-headphones-symbolic");
        assert_eq!(mapped.device_type, "Headphones");
        assert!(mapped.connected);
        assert_eq!(mapped.battery, Some(75));
    }
}
