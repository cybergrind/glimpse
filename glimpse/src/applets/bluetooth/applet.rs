use glimpse::{
    bluetooth::provider::{BluetoothDevice, BluetoothSnapshot},
    bluetooth::{
        BluetoothServiceHandle,
        protocol::{
            BluetoothActiveAction, BluetoothPrompt, BluetoothPromptKind, BluetoothServiceCommand,
            BluetoothServiceState,
        },
    },
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use std::process::Command;

use super::BluetoothConfig;
use super::components::{
    BluetoothPromptDialog, BluetoothPromptDialogInit, BluetoothPromptDialogInput,
    BluetoothPromptDialogOutput,
};
use super::popover::{
    BluetoothDeviceAction, BluetoothPopover, BluetoothPopoverInit, BluetoothPopoverInput,
    BluetoothPopoverOutput, BtDevice,
};
pub struct Bluetooth {
    icon_name: String,
    tooltip: String,
    service: BluetoothServiceHandle,
    settings_command: String,
    active_device: Option<String>,
    popover: Controller<BluetoothPopover>,
    prompt_dialog: Controller<BluetoothPromptDialog>,
}

pub struct BluetoothInit {
    pub config: BluetoothConfig,
    pub service: BluetoothServiceHandle,
}

#[derive(Debug, Clone)]
pub enum BluetoothMsg {
    ServiceState(BluetoothServiceState),
    PopoverOutput(BluetoothPopoverOutput),
    PromptDialogOutput(BluetoothPromptDialogOutput),
    Reconfigure(BluetoothConfig),
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
        let settings_command = init.config.settings_command.clone();
        let popover = BluetoothPopover::builder()
            .launch(BluetoothPopoverInit {
                parent: root.clone(),
                show_settings_button: !settings_command.is_empty(),
            })
            .forward(sender.input_sender(), BluetoothMsg::PopoverOutput);
        let prompt_dialog = BluetoothPromptDialog::builder()
            .launch(BluetoothPromptDialogInit {
                parent: root.clone().upcast(),
            })
            .forward(sender.input_sender(), BluetoothMsg::PromptDialogOutput);

        let model = Bluetooth {
            icon_name: "bluetooth-active-symbolic".into(),
            tooltip: "Bluetooth".into(),
            service: init.service.clone(),
            settings_command,
            active_device: None,
            popover,
            prompt_dialog,
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
                self.prompt_dialog.emit(BluetoothPromptDialogInput::Update {
                    prompt: state.prompt.clone(),
                    snapshot: state.snapshot.clone(),
                });
                self.sync_activity(&state, &sender, root);
            }
            BluetoothMsg::PopoverOutput(output) => {
                self.handle_popover_output(output, sender);
            }
            BluetoothMsg::PromptDialogOutput(BluetoothPromptDialogOutput::Reply { id, reply }) => {
                self.send_command(sender, BluetoothServiceCommand::PromptReply { id, reply });
            }
            BluetoothMsg::Reconfigure(config) => {
                self.settings_command = config.settings_command;
                self.popover.emit(BluetoothPopoverInput::SetShowSettingsButton(
                    !self.settings_command.is_empty(),
                ));
            }
            BluetoothMsg::TogglePopover => {
                self.popover.emit(BluetoothPopoverInput::Toggle);
            }
            BluetoothMsg::Unavailable => {
                tracing::warn!("bluetooth applet: bluetooth service unavailable");
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
            BluetoothPopoverOutput::OpenSettings => {
                tracing::info!("bluetooth applet: open settings requested");
                self.popover.emit(BluetoothPopoverInput::Close);
                self.open_settings();
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

    fn open_settings(&self) {
        if self.settings_command.is_empty() {
            tracing::debug!("bluetooth applet: ignoring settings request with empty command");
            return;
        }

        let command = self.settings_command.clone();
        if let Ok(mut child) = Command::new("sh").arg("-c").arg(&command).spawn() {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        } else {
            tracing::warn!("bluetooth applet: failed to spawn bluetooth settings command");
        }
    }

    fn sync_activity(
        &mut self,
        state: &BluetoothServiceState,
        _sender: &ComponentSender<Self>,
        _root: &gtk::Box,
    ) {
        let next_active_device = active_device_address(state.active_action.as_ref());

        if let Some(previous) = self.active_device.take() {
            if next_active_device.as_ref() != Some(&previous) {
                self.popover
                    .emit(BluetoothPopoverInput::FinishDeviceAction { address: previous });
            }
        }

        if let Some(prompt) = state.prompt.as_ref() {
            self.popover.emit(BluetoothPopoverInput::SetActivity(Some(
                prompt_activity_status(prompt, &state.snapshot),
            )));
        } else {
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
    use glimpse::bluetooth::protocol::{
        BluetoothPromptId, BluetoothPromptKind, BluetoothServiceHealth,
    };

    #[test]
    fn popover_device_uses_provider_type_metadata() {
        let device = BluetoothDevice {
            path: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".into(),
            address: "AA:BB:CC:DD:EE:FF".into(),
            alias: "Headphones".into(),
            name: "Headphones".into(),
            device_type: glimpse::bluetooth::provider::BluetoothDeviceType::Headphones,
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

    #[test]
    fn pairing_confirm_prompt_uses_activity_text() {
        let snapshot = BluetoothSnapshot {
            status: Default::default(),
            adapters: vec![],
            devices: vec![BluetoothDevice {
                path: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".into(),
                address: "AA:BB:CC:DD:EE:FF".into(),
                alias: "Headphones".into(),
                name: "Headphones".into(),
                device_type: glimpse::bluetooth::provider::BluetoothDeviceType::Headphones,
                paired: false,
                connected: false,
                trusted: false,
                battery: None,
                rssi: None,
                class: 0,
                appearance: 0,
                adapter: "/org/bluez/hci0".into(),
            }],
        };
        let prompt = BluetoothPrompt {
            id: BluetoothPromptId(7),
            device_path: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".into(),
            device_label: "Headphones".into(),
            kind: BluetoothPromptKind::Confirm { passkey: 123456 },
        };

        assert_eq!(
            prompt_activity_status(&prompt, &snapshot),
            "Confirm pairing with Headphones"
        );
    }
}
