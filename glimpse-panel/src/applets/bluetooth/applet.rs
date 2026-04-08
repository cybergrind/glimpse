use glimpse::providers::bluetooth::{
    BluetoothChangeReason, BluetoothDevice, BluetoothProvider, BluetoothProviderEvent,
    BluetoothSnapshot,
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::config::BluetoothConfig;
use super::popover::{
    BluetoothDeviceAction, BluetoothPopover, BluetoothPopoverInit, BluetoothPopoverInput,
    BluetoothPopoverOutput, BtDevice,
};

pub struct Bluetooth {
    icon_name: String,
    tooltip: String,
    action_tx: mpsc::Sender<BluetoothActionRequest>,
    popover: Controller<BluetoothPopover>,
}

pub struct BluetoothInit {
    pub config: BluetoothConfig,
    pub conn: zbus::Connection,
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
    PopoverOutput(BluetoothPopoverOutput),
    DeviceActionFinished { address: String },
    TogglePopover,
    Unavailable,
}

#[derive(Debug)]
enum BluetoothActionRequest {
    StartDiscovery,
    StopDiscovery,
    SetPowered(bool),
    DeviceAction {
        address: String,
        name: String,
        action: BluetoothDeviceAction,
    },
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
        let (action_tx, mut action_rx) = mpsc::channel::<BluetoothActionRequest>(16);
        let popover = BluetoothPopover::builder()
            .launch(BluetoothPopoverInit {
                parent: root.clone(),
                settings_command: init.config.settings_command,
            })
            .forward(sender.input_sender(), BluetoothMsg::PopoverOutput);

        let model = Bluetooth {
            icon_name: "bluetooth-active-symbolic".into(),
            tooltip: "Bluetooth".into(),
            action_tx,
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

                    loop {
                        tokio::select! {
                            Some(event) = event_rx.recv() => {
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
                            Some(action) = action_rx.recv() => {
                                handle_action(&provider, action, &out).await;
                            }
                            else => break,
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
                } else if connected_count > 0 {
                    "bluetooth-active-symbolic"
                } else {
                    "bluetooth-symbolic"
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
            BluetoothMsg::PopoverOutput(output) => {
                self.handle_popover_output(output);
            }
            BluetoothMsg::DeviceActionFinished { address } => {
                self.popover
                    .emit(BluetoothPopoverInput::FinishDeviceAction { address });
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

impl Bluetooth {
    fn handle_popover_output(&self, output: BluetoothPopoverOutput) {
        match output {
            BluetoothPopoverOutput::Opened => {
                tracing::info!("bluetooth applet: popover opened");
                queue_action(&self.action_tx, BluetoothActionRequest::StartDiscovery);
            }
            BluetoothPopoverOutput::Closed => {
                tracing::info!("bluetooth applet: popover closed");
                queue_action(&self.action_tx, BluetoothActionRequest::StopDiscovery);
            }
            BluetoothPopoverOutput::SetPowered(powered) => {
                tracing::info!(powered, "bluetooth applet: set power requested");
                queue_action(&self.action_tx, BluetoothActionRequest::SetPowered(powered));
            }
            BluetoothPopoverOutput::DeviceAction {
                address,
                name,
                action,
            } => {
                tracing::info!(?action, address = %address, name = %name, "bluetooth applet: device action requested");
                queue_action(
                    &self.action_tx,
                    BluetoothActionRequest::DeviceAction {
                        address,
                        name,
                        action,
                    },
                );
            }
        }
    }
}

fn queue_action(action_tx: &mpsc::Sender<BluetoothActionRequest>, action: BluetoothActionRequest) {
    if let Err(error) = action_tx.try_send(action) {
        tracing::warn!(error = %error, "bluetooth applet: failed to queue action");
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

async fn handle_action(
    provider: &BluetoothProvider,
    action: BluetoothActionRequest,
    out: &relm4::Sender<BluetoothMsg>,
) {
    match action {
        BluetoothActionRequest::StartDiscovery => {
            if let Err(error) = provider.start_discovery().await {
                tracing::warn!(error = %error, "bluetooth applet: start discovery failed");
            }
        }
        BluetoothActionRequest::StopDiscovery => {
            if let Err(error) = provider.stop_discovery().await {
                tracing::warn!(error = %error, "bluetooth applet: stop discovery failed");
            }
        }
        BluetoothActionRequest::SetPowered(powered) => match provider.set_powered(powered).await {
            Ok(()) => tracing::info!(powered, "bluetooth applet: set power succeeded"),
            Err(error) => tracing::warn!(powered, error = %error, "bluetooth applet: set power failed"),
        },
        BluetoothActionRequest::DeviceAction {
            address,
            name,
            action,
        } => {
            let result = match action {
                BluetoothDeviceAction::Connect => provider.connect(&address).await,
                BluetoothDeviceAction::Disconnect => provider.disconnect(&address).await,
                BluetoothDeviceAction::Pair => provider.pair(&address).await,
                BluetoothDeviceAction::Forget => provider.forget(&address).await,
            };

            match result {
                Ok(()) => tracing::info!(?action, address = %address, name = %name, "bluetooth applet: device action succeeded"),
                Err(error) => tracing::warn!(?action, address = %address, name = %name, error = %error, "bluetooth applet: device action failed"),
            }

            let _ = out.send(BluetoothMsg::DeviceActionFinished { address });
        }
    }
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
