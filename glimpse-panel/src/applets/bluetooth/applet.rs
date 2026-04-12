use adw::prelude::{AdwDialogExt, AlertDialogExt};
use glimpse::{
    bluetooth::{
        BluetoothServiceHandle,
        protocol::{
            BluetoothActiveAction, BluetoothPrompt, BluetoothPromptId, BluetoothPromptKind,
            BluetoothPromptReply, BluetoothServiceCommand, BluetoothServiceState,
        },
    },
    providers::bluetooth::{BluetoothDevice, BluetoothSnapshot},
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use super::config::BluetoothConfig;
use super::popover::{
    BluetoothDeviceAction, BluetoothPopover, BluetoothPopoverInit, BluetoothPopoverInput,
    BluetoothPopoverOutput, BtDevice,
};
pub struct Bluetooth {
    icon_name: String,
    tooltip: String,
    service: BluetoothServiceHandle,
    active_device: Option<String>,
    active_prompt_id: Option<BluetoothPromptId>,
    popover: Controller<BluetoothPopover>,
}

pub struct BluetoothInit {
    pub config: BluetoothConfig,
    pub service: BluetoothServiceHandle,
}

#[derive(Debug, Clone)]
pub enum BluetoothMsg {
    ServiceState(BluetoothServiceState),
    PopoverOutput(BluetoothPopoverOutput),
    TogglePopover,
    Unavailable,
    PromptReply {
        id: BluetoothPromptId,
        reply: BluetoothPromptReply,
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
        let popover = BluetoothPopover::builder()
            .launch(BluetoothPopoverInit {
                parent: root.clone(),
                settings_command: init.config.settings_command,
            })
            .forward(sender.input_sender(), BluetoothMsg::PopoverOutput);

        let model = Bluetooth {
            icon_name: "bluetooth-active-symbolic".into(),
            tooltip: "Bluetooth".into(),
            service: init.service.clone(),
            active_device: None,
            active_prompt_id: None,
            popover,
        };

        let service = init.service;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("bluetooth applet: subscribing to bluetooth service");
                    let mut state_rx = service.subscribe();
                    let _ = out.send(BluetoothMsg::ServiceState(state_rx.borrow().clone()));

                    loop {
                        if state_rx.changed().await.is_err() {
                            break;
                        }
                        let _ = out.send(BluetoothMsg::ServiceState(state_rx.borrow().clone()));
                    }

                    tracing::warn!("bluetooth applet: service state channel closed");
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

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            BluetoothMsg::ServiceState(state) => {
                let powered = state.snapshot.status.powered;
                let discovering = state.snapshot.status.discovering;
                let connected_count = state.snapshot.status.connected_count;
                tracing::debug!(
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
                self.popover
                    .emit(BluetoothPopoverInput::UpdateDevices(popover_devices(
                        state.snapshot.clone(),
                    )));
                self.sync_activity(&state, &sender, root);
            }
            BluetoothMsg::PopoverOutput(output) => {
                self.handle_popover_output(output, sender);
            }
            BluetoothMsg::TogglePopover => {
                self.popover.emit(BluetoothPopoverInput::Toggle);
            }
            BluetoothMsg::Unavailable => {
                tracing::warn!("bluetooth applet: bluetooth service unavailable");
            }
            BluetoothMsg::PromptReply { id, reply } => {
                tracing::info!(prompt_id = id.0, "bluetooth applet: prompt reply");
                self.send_command(sender, BluetoothServiceCommand::PromptReply { id, reply });
            }
        }
    }
}

impl Bluetooth {
    fn handle_popover_output(
        &self,
        output: BluetoothPopoverOutput,
        sender: ComponentSender<Bluetooth>,
    ) {
        match output {
            BluetoothPopoverOutput::Opened => {
                tracing::info!("bluetooth applet: popover opened");
                self.send_command(sender, BluetoothServiceCommand::StartDiscovery);
            }
            BluetoothPopoverOutput::Closed => {
                tracing::info!("bluetooth applet: popover closed");
                self.send_command(sender, BluetoothServiceCommand::StopDiscovery);
            }
            BluetoothPopoverOutput::SetPowered(powered) => {
                tracing::info!(powered, "bluetooth applet: set power requested");
                self.send_command(sender, BluetoothServiceCommand::SetPowered(powered));
            }
            BluetoothPopoverOutput::DeviceAction {
                address,
                name,
                action,
            } => {
                tracing::info!(?action, address = %address, name = %name, "bluetooth applet: device action requested");
                self.send_command(sender, command_for_device_action(action, address));
            }
        }
    }

    fn send_command(&self, sender: ComponentSender<Bluetooth>, command: BluetoothServiceCommand) {
        let service = self.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    if let Err(error) = service.send(command).await {
                        tracing::warn!(error = %error, "bluetooth applet: failed to send bluetooth service command");
                    }
                })
                .drop_on_shutdown()
        });
    }

    fn sync_activity(
        &mut self,
        state: &BluetoothServiceState,
        sender: &ComponentSender<Self>,
        root: &gtk::Box,
    ) {
        let next_active_device = active_device_address(state.active_action.as_ref());

        if let Some(previous) = self.active_device.take() {
            if next_active_device.as_ref() != Some(&previous) {
                self.popover
                    .emit(BluetoothPopoverInput::FinishDeviceAction { address: previous });
            }
        }

        if let Some(prompt) = state.prompt.as_ref() {
            if let BluetoothPromptKind::Confirm { passkey } = &prompt.kind {
                if self.active_prompt_id != Some(prompt.id) {
                    self.active_prompt_id = Some(prompt.id);
                    self.show_confirm_dialog(
                        root,
                        sender,
                        prompt.id,
                        *passkey,
                        prompt.device_label.clone(),
                    );
                }
                self.popover.emit(BluetoothPopoverInput::SetActivity(None));
            } else {
                self.active_prompt_id = None;
                self.popover.emit(BluetoothPopoverInput::SetActivity(Some(
                    prompt_activity_status(prompt, &state.snapshot),
                )));
            }
        } else {
            self.active_prompt_id = None;
            if state.active_action.is_some() {
                self.popover.emit(BluetoothPopoverInput::SetActivity(Some(
                    service_activity_status(state),
                )));
            } else {
                self.popover.emit(BluetoothPopoverInput::SetActivity(None));
            }
        }

        self.active_device = next_active_device;
    }

    fn show_confirm_dialog(
        &self,
        root: &gtk::Box,
        sender: &ComponentSender<Self>,
        id: BluetoothPromptId,
        passkey: u32,
        device_label: String,
    ) {
        let dialog = adw::AlertDialog::new(
            Some("Confirm Bluetooth Pairing"),
            Some(&format!(
                "Confirm pairing with {}?\n\nPasskey: {:06}",
                device_label, passkey
            )),
        );
        dialog.add_response("reject", "Reject");
        dialog.add_response("confirm", "Confirm");
        dialog.set_response_appearance("confirm", adw::ResponseAppearance::Suggested);
        dialog.set_response_appearance("reject", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("confirm"));
        dialog.set_close_response("reject");

        let sender = sender.clone();
        dialog.connect_response(None, move |_, response| {
            let reply = if response == "confirm" {
                BluetoothPromptReply::Confirm
            } else {
                BluetoothPromptReply::Cancel
            };
            sender.input(BluetoothMsg::PromptReply { id, reply });
        });

        dialog.present(Some(root));
    }
}

fn command_for_device_action(
    action: BluetoothDeviceAction,
    address: String,
) -> BluetoothServiceCommand {
    match action {
        BluetoothDeviceAction::Connect => BluetoothServiceCommand::Connect { address },
        BluetoothDeviceAction::Disconnect => BluetoothServiceCommand::Disconnect { address },
        BluetoothDeviceAction::Pair => BluetoothServiceCommand::Pair { address },
        BluetoothDeviceAction::Trust(trusted) => {
            BluetoothServiceCommand::Trust { address, trusted }
        }
        BluetoothDeviceAction::Forget => BluetoothServiceCommand::Forget { address },
    }
}

fn service_activity_status(state: &BluetoothServiceState) -> String {
    match state.active_action.as_ref() {
        Some(BluetoothActiveAction::SetPowered(true)) => "Turning Bluetooth on...".into(),
        Some(BluetoothActiveAction::SetPowered(false)) => "Turning Bluetooth off...".into(),
        Some(BluetoothActiveAction::SetAdapterPowered { powered: true, .. }) => {
            "Turning adapter on...".into()
        }
        Some(BluetoothActiveAction::SetAdapterPowered { powered: false, .. }) => {
            "Turning adapter off...".into()
        }
        Some(BluetoothActiveAction::SetAdapterDiscoverable {
            discoverable: true, ..
        }) => "Making adapter discoverable...".into(),
        Some(BluetoothActiveAction::SetAdapterDiscoverable {
            discoverable: false,
            ..
        }) => "Hiding adapter...".into(),
        Some(BluetoothActiveAction::Connect { address }) => {
            format!("Connecting {}...", device_name(&state.snapshot, address))
        }
        Some(BluetoothActiveAction::Disconnect { address }) => {
            format!("Disconnecting {}...", device_name(&state.snapshot, address))
        }
        Some(BluetoothActiveAction::Pair { address }) => {
            format!("Pairing {}...", device_name(&state.snapshot, address))
        }
        Some(BluetoothActiveAction::Trust { address, trusted }) => {
            if *trusted {
                format!("Trusting {}...", device_name(&state.snapshot, address))
            } else {
                format!("Untrusting {}...", device_name(&state.snapshot, address))
            }
        }
        Some(BluetoothActiveAction::Forget { address }) => {
            format!("Forgetting {}...", device_name(&state.snapshot, address))
        }
        None => String::new(),
    }
}

fn active_device_address(action: Option<&BluetoothActiveAction>) -> Option<String> {
    match action {
        Some(BluetoothActiveAction::Connect { address })
        | Some(BluetoothActiveAction::Disconnect { address })
        | Some(BluetoothActiveAction::Pair { address })
        | Some(BluetoothActiveAction::Trust { address, .. })
        | Some(BluetoothActiveAction::Forget { address }) => Some(address.clone()),
        Some(BluetoothActiveAction::SetPowered(_))
        | Some(BluetoothActiveAction::SetAdapterPowered { .. })
        | Some(BluetoothActiveAction::SetAdapterDiscoverable { .. })
        | None => None,
    }
}

fn device_name(snapshot: &BluetoothSnapshot, address: &str) -> String {
    snapshot
        .devices
        .iter()
        .find(|device| device.address == address)
        .map(|device| device.name.clone())
        .unwrap_or_else(|| address.to_owned())
}

fn prompt_activity_status(prompt: &BluetoothPrompt, snapshot: &BluetoothSnapshot) -> String {
    let label = prompt_device_label(prompt, snapshot);
    match &prompt.kind {
        BluetoothPromptKind::Confirm { .. } => format!("Confirm pairing with {label}"),
        BluetoothPromptKind::RequestPin => format!("Enter the PIN for {label}"),
        BluetoothPromptKind::RequestPasskey => format!("Enter the passkey for {label}"),
        BluetoothPromptKind::DisplayPin { .. } => format!("Type the PIN on {label}"),
        BluetoothPromptKind::DisplayPasskey { .. } => format!("Type the passkey on {label}"),
    }
}

fn prompt_device_label(prompt: &BluetoothPrompt, snapshot: &BluetoothSnapshot) -> String {
    if !prompt.device_label.is_empty() && prompt.device_label != prompt.device_path {
        return prompt.device_label.clone();
    }

    if let Some(address) = address_from_device_path(&prompt.device_path) {
        return device_name(snapshot, &address);
    }

    prompt.device_path.clone()
}

fn address_from_device_path(path: &str) -> Option<String> {
    let tail = path.rsplit('/').next()?;
    let suffix = tail.strip_prefix("dev_")?;
    Some(suffix.replace('_', ":"))
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
    use glimpse::bluetooth::protocol::BluetoothServiceHealth;

    #[test]
    fn popover_device_uses_provider_type_metadata() {
        let device = BluetoothDevice {
            path: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".into(),
            address: "AA:BB:CC:DD:EE:FF".into(),
            alias: "Headphones".into(),
            name: "Headphones".into(),
            device_type: glimpse::providers::bluetooth::BluetoothDeviceType::Headphones,
            paired: true,
            connected: true,
            trusted: true,
            battery: Some(75),
            rssi: Some(-30),
            class: 0,
            appearance: 0,
            adapter: "/org/bluez/hci0".into(),
        };

        let mapped = popover_device(device);

        assert_eq!(mapped.icon, "audio-headphones-symbolic");
        assert_eq!(mapped.device_type, "Headphones");
        assert!(mapped.connected);
        assert_eq!(mapped.battery, Some(75));
    }

    #[test]
    fn adapter_actions_do_not_target_a_device() {
        assert_eq!(
            active_device_address(Some(&BluetoothActiveAction::SetAdapterPowered {
                adapter_path: "/org/bluez/hci1".into(),
                powered: true,
            })),
            None
        );
        assert_eq!(
            active_device_address(Some(&BluetoothActiveAction::SetAdapterDiscoverable {
                adapter_path: "/org/bluez/hci1".into(),
                discoverable: true,
            })),
            None
        );
    }

    #[test]
    fn adapter_actions_have_panel_status_messages() {
        let state = BluetoothServiceState {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot::default(),
            prompt: None,
            active_action: Some(BluetoothActiveAction::SetAdapterDiscoverable {
                adapter_path: "/org/bluez/hci1".into(),
                discoverable: true,
            }),
        };

        assert_eq!(
            service_activity_status(&state),
            "Making adapter discoverable..."
        );
    }
}
