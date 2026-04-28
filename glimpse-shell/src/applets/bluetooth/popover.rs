#![allow(unused_assignments)]

use std::cell::Cell;
use std::rc::Rc;

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use crate::{
    components::{
        device_list::{DeviceList, DeviceListInit, DeviceListInput, DeviceListItem},
        hero::HeroView,
        popover_shell::PopoverShell,
    },
    services::bluetooth::{
        BluetoothActiveAction, BluetoothDevice, BluetoothServiceHealth, BluetoothSnapshot, Command,
        State,
    },
};

use super::format;

pub struct Popover {
    popover: gtk::Popover,
    hero_icon_name: String,
    hero_subtitle: String,
    powered: bool,
    updating_power: Rc<Cell<bool>>,
    devices: Controller<DeviceList<Command>>,
    device_items: Vec<DeviceListItem<Command>>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    SetPowered(bool),
    DeviceCommand(Command),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PopoverOutput {
    Opened,
    Closed,
    Command(Command),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "bluetooth-popover",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[name = "hero"]
                    #[template]
                    HeroView {
                        #[template_child]
                        trailing {
                            set_visible: true,
                        },
                    },

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    #[local_ref]
                    devices_widget -> gtk::Box {},
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let devices = DeviceList::builder()
            .launch(DeviceListInit {
                header: None,
                items: Vec::new(),
            })
            .forward(sender.input_sender(), PopoverInput::DeviceCommand);
        let devices_widget = devices.widget().clone();

        let updating_power = Rc::new(Cell::new(false));
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        devices.widget().set_visible(false);

        let toggle_guard = updating_power.clone();
        let toggle_sender = sender.clone();
        widgets.hero.toggle.connect_state_set(move |_, active| {
            if toggle_guard.get() {
                return glib::Propagation::Stop;
            }

            toggle_sender.input(PopoverInput::SetPowered(active));
            glib::Propagation::Stop
        });

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(PopoverOutput::Opened);
        });

        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(PopoverOutput::Closed);
        });

        let model = Popover {
            popover: widgets.root.clone(),
            hero_icon_name: "bluetooth-disabled-symbolic".into(),
            hero_subtitle: "Off".into(),
            powered: false,
            updating_power,
            devices,
            device_items: Vec::new(),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            PopoverInput::UpdateState(state) => {
                self.powered = state.snapshot.status.powered;
                self.hero_icon_name = hero_icon_name_for_state(&state).into();
                self.hero_subtitle = hero_subtitle_text(&state);

                let items = build_device_items(&state);
                if self.device_items != items {
                    self.devices.widget().set_visible(!items.is_empty());
                    self.devices.emit(DeviceListInput::Update(items.clone()));
                    self.device_items = items;
                }
            }
            PopoverInput::SetPowered(powered) => {
                let _ = sender.output(PopoverOutput::Command(Command::SetPowered(powered)));
            }
            PopoverInput::DeviceCommand(command) => {
                if let Some(address) = optimistic_busy_address(&command).map(str::to_owned) {
                    if mark_device_busy(&mut self.device_items, &address) {
                        self.devices
                            .emit(DeviceListInput::Update(self.device_items.clone()));
                    }
                }
                let _ = sender.output(PopoverOutput::Command(command));
            }
        }
    }

    fn post_view() {
        hero.icon.set_icon_name(Some(&model.hero_icon_name));
        hero.title.set_label("Bluetooth");
        hero.subtitle.set_label(&model.hero_subtitle);

        if hero.toggle.is_active() != model.powered {
            model.updating_power.set(true);
            hero.toggle.set_active(model.powered);
            hero.toggle.set_state(model.powered);
            model.updating_power.set(false);
        }
    }
}

fn hero_icon_name_for_state(state: &State) -> &'static str {
    if !state.snapshot.status.powered {
        "bluetooth-disabled-symbolic"
    } else if state.snapshot.status.connected_count > 0 {
        "bluetooth-active-symbolic"
    } else {
        "bluetooth-symbolic"
    }
}

fn hero_subtitle_text(state: &State) -> String {
    match &state.health {
        BluetoothServiceHealth::Starting => return "Starting".into(),
        BluetoothServiceHealth::Reconnecting { .. } => return "Reconnecting".into(),
        BluetoothServiceHealth::Degraded { message } => return message.clone(),
        BluetoothServiceHealth::Ready => {}
    }

    if let Some(prompt) = &state.prompt {
        return format::prompt_activity_text(prompt, &state.snapshot);
    }

    if let Some(activity) = active_action_text(state) {
        return activity;
    }

    let status = &state.snapshot.status;
    if !status.powered {
        "Off".into()
    } else if status.discovering {
        "Discovering".into()
    } else if status.connected_count > 0 {
        format!("{} connected", status.connected_count)
    } else {
        "Ready".into()
    }
}

fn active_action_text(state: &State) -> Option<String> {
    match state.active_action.as_ref()? {
        BluetoothActiveAction::SetPowered(true) => Some("Turning Bluetooth on".into()),
        BluetoothActiveAction::SetPowered(false) => Some("Turning Bluetooth off".into()),
        BluetoothActiveAction::SetAdapterPowered { powered: true, .. } => {
            Some("Turning adapter on".into())
        }
        BluetoothActiveAction::SetAdapterPowered { powered: false, .. } => {
            Some("Turning adapter off".into())
        }
        BluetoothActiveAction::SetAdapterDiscoverable {
            discoverable: true, ..
        } => Some("Making adapter discoverable".into()),
        BluetoothActiveAction::SetAdapterDiscoverable {
            discoverable: false,
            ..
        } => Some("Hiding adapter".into()),
        BluetoothActiveAction::Connect { address } => Some(format!(
            "Connecting {}",
            device_name(&state.snapshot, address)
        )),
        BluetoothActiveAction::Disconnect { address } => Some(format!(
            "Disconnecting {}",
            device_name(&state.snapshot, address)
        )),
        BluetoothActiveAction::Pair { address } => {
            Some(format!("Pairing {}", device_name(&state.snapshot, address)))
        }
        BluetoothActiveAction::Trust { address, trusted } => {
            if *trusted {
                Some(format!(
                    "Trusting {}",
                    device_name(&state.snapshot, address)
                ))
            } else {
                Some(format!(
                    "Untrusting {}",
                    device_name(&state.snapshot, address)
                ))
            }
        }
        BluetoothActiveAction::Forget { address } => Some(format!(
            "Forgetting {}",
            device_name(&state.snapshot, address)
        )),
    }
}

fn build_device_items(state: &State) -> Vec<DeviceListItem<Command>> {
    let busy_address = busy_device_address(state);

    visible_devices(&state.snapshot)
        .into_iter()
        .map(|device| DeviceListItem {
            id: device.address.clone(),
            label: device.name.clone(),
            icon: device.device_type.icon(device.connected).into(),
            status: device_status(device),
            busy: busy_address == Some(device.address.as_str()),
            tooltip: Some(device_tooltip(device)),
            active: device.connected,
            visible: true,
            command: Some(primary_device_command(device)),
        })
        .collect()
}

fn busy_device_address(state: &State) -> Option<&str> {
    match state.active_action.as_ref()? {
        BluetoothActiveAction::Connect { address } => Some(address.as_str()),
        BluetoothActiveAction::Pair { address } => Some(address.as_str()),
        _ => None,
    }
}

fn optimistic_busy_address(command: &Command) -> Option<&str> {
    match command {
        Command::Connect { address } | Command::Pair { address } => Some(address.as_str()),
        _ => None,
    }
}

fn mark_device_busy(items: &mut [DeviceListItem<Command>], address: &str) -> bool {
    let mut changed = false;
    for item in items {
        let busy = item.id == address;
        if item.busy != busy {
            item.busy = busy;
            changed = true;
        }
    }
    changed
}

fn visible_devices(snapshot: &BluetoothSnapshot) -> Vec<&BluetoothDevice> {
    let mut devices = snapshot
        .devices
        .iter()
        .filter(|device| is_visible_device(device))
        .collect::<Vec<_>>();
    devices.sort_by(|left, right| {
        right
            .connected
            .cmp(&left.connected)
            .then(right.paired.cmp(&left.paired))
            .then(
                right
                    .rssi
                    .unwrap_or(i16::MIN)
                    .cmp(&left.rssi.unwrap_or(i16::MIN)),
            )
            .then(left.name.cmp(&right.name))
    });
    devices
}

fn is_visible_device(device: &BluetoothDevice) -> bool {
    if device.address.is_empty() {
        return false;
    }

    if device.name.is_empty() || looks_like_mac(&device.name) {
        return device.connected || device.paired || device.trusted;
    }

    device.connected || device.paired || device.trusted || device.rssi.is_some()
}

fn primary_device_command(device: &BluetoothDevice) -> Command {
    if device.connected {
        Command::Disconnect {
            address: device.address.clone(),
        }
    } else if device.paired {
        Command::Connect {
            address: device.address.clone(),
        }
    } else {
        Command::Pair {
            address: device.address.clone(),
        }
    }
}

fn device_status(device: &BluetoothDevice) -> String {
    device
        .battery
        .map(|percentage| format!("{percentage}%"))
        .unwrap_or_default()
}

fn device_tooltip(device: &BluetoothDevice) -> String {
    let mut parts = Vec::new();
    let device_type = device.device_type.label();
    if !device_type.is_empty() {
        parts.push(device_type.to_owned());
    }
    if device.connected {
        parts.push("Connected".into());
    } else if device.paired {
        parts.push("Paired".into());
    }
    if parts.is_empty() {
        device.name.clone()
    } else {
        parts.join(" \u{b7} ")
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

fn looks_like_mac(value: &str) -> bool {
    let value = value.trim();
    if value.len() != 17 {
        return false;
    }

    let separator = if value.contains(':') {
        ':'
    } else if value.contains('-') {
        '-'
    } else {
        return false;
    };

    let parts = value.split(separator).collect::<Vec<_>>();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|char| char.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::bluetooth::{BluetoothDeviceType, BluetoothStatus};

    fn device(address: &str, name: &str, connected: bool, paired: bool) -> BluetoothDevice {
        BluetoothDevice {
            path: format!("/org/bluez/hci0/dev_{}", address.replace(':', "_")),
            address: address.into(),
            alias: name.into(),
            name: name.into(),
            device_type: BluetoothDeviceType::Unknown,
            paired,
            connected,
            trusted: false,
            battery: None,
            rssi: Some(-30),
            class: 0,
            appearance: 0,
            adapter: "/org/bluez/hci0".into(),
        }
    }

    #[test]
    fn primary_device_command_matches_device_state() {
        assert_eq!(
            primary_device_command(&device("AA:BB", "Headphones", true, true)),
            Command::Disconnect {
                address: "AA:BB".into()
            }
        );
        assert_eq!(
            primary_device_command(&device("AA:BB", "Headphones", false, true)),
            Command::Connect {
                address: "AA:BB".into()
            }
        );
        assert_eq!(
            primary_device_command(&device("AA:BB", "Headphones", false, false)),
            Command::Pair {
                address: "AA:BB".into()
            }
        );
    }

    #[test]
    fn hero_subtitle_prefers_health_then_activity_then_status() {
        let mut state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus {
                    powered: true,
                    discovering: true,
                    connected_count: 0,
                },
                devices: vec![],
                adapters: vec![],
            },
            prompt: None,
            active_action: None,
        };

        assert_eq!(hero_subtitle_text(&state), "Discovering");

        state.active_action = Some(BluetoothActiveAction::SetPowered(false));
        assert_eq!(hero_subtitle_text(&state), "Turning Bluetooth off");

        state.health = BluetoothServiceHealth::Reconnecting { attempt: 2 };
        assert_eq!(hero_subtitle_text(&state), "Reconnecting");
    }

    #[test]
    fn device_items_hide_raw_uninteresting_addresses() {
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![
                    device("AA:BB:CC:DD:EE:01", "AA:BB:CC:DD:EE:01", false, false),
                    device("AA:BB:CC:DD:EE:02", "Mouse", false, false),
                ],
            },
            prompt: None,
            active_action: None,
        };

        let items = build_device_items(&state);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "Mouse");
    }

    #[test]
    fn device_item_status_is_battery_percentage_when_available() {
        let mut device = device("AA:BB:CC:DD:EE:02", "Mouse", true, true);
        device.battery = Some(75);
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![device],
            },
            prompt: None,
            active_action: None,
        };

        let items = build_device_items(&state);

        assert_eq!(items[0].status, "75%");
        assert!(!items[0].busy);
        assert!(items[0].active);
    }

    #[test]
    fn connecting_device_item_sets_busy_status_slot() {
        let device = device("AA:BB:CC:DD:EE:02", "Mouse", false, true);
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![device],
            },
            prompt: None,
            active_action: Some(BluetoothActiveAction::Connect {
                address: "AA:BB:CC:DD:EE:02".into(),
            }),
        };

        let items = build_device_items(&state);

        assert!(items[0].busy);
    }

    #[test]
    fn pairing_device_item_sets_busy_status_slot() {
        let device = device("AA:BB:CC:DD:EE:02", "Mouse", false, false);
        let state = State {
            health: BluetoothServiceHealth::Ready,
            snapshot: BluetoothSnapshot {
                status: BluetoothStatus::default(),
                adapters: vec![],
                devices: vec![device],
            },
            prompt: None,
            active_action: Some(BluetoothActiveAction::Pair {
                address: "AA:BB:CC:DD:EE:02".into(),
            }),
        };

        let items = build_device_items(&state);

        assert!(items[0].busy);
    }

    #[test]
    fn optimistic_busy_marks_clicked_pair_or_connect_device() {
        assert_eq!(
            optimistic_busy_address(&Command::Pair {
                address: "AA:BB".into()
            }),
            Some("AA:BB")
        );
        assert_eq!(
            optimistic_busy_address(&Command::Connect {
                address: "AA:BB".into()
            }),
            Some("AA:BB")
        );
        assert_eq!(optimistic_busy_address(&Command::SetPowered(true)), None);

        let mut items = vec![
            DeviceListItem {
                id: "AA:BB".into(),
                icon: String::new(),
                label: "Headphones".into(),
                status: String::new(),
                busy: false,
                tooltip: None,
                active: false,
                visible: true,
                command: Some(Command::Pair {
                    address: "AA:BB".into(),
                }),
            },
            DeviceListItem {
                id: "CC:DD".into(),
                icon: String::new(),
                label: "Mouse".into(),
                status: String::new(),
                busy: true,
                tooltip: None,
                active: false,
                visible: true,
                command: Some(Command::Pair {
                    address: "CC:DD".into(),
                }),
            },
        ];

        assert!(mark_device_busy(&mut items, "AA:BB"));
        assert!(items[0].busy);
        assert!(!items[1].busy);
    }
}
