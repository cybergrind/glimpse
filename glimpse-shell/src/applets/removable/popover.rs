use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::{
        animated_popover::AnimatedPopover,
        device_list::{DeviceList, DeviceListInit, DeviceListInput, DeviceListItem},
        popover_scroll,
        popover_shell::PopoverShell,
    },
    services::storage::{Command, State, StorageDevice},
};

pub struct Popover {
    animation: AnimatedPopover,
    devices: Controller<DeviceList<Command>>,
    device_items: Vec<DeviceListItem<Command>>,
    empty_label: gtk::Label,
}

#[derive(Debug)]
pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    DeviceCommand(Command),
}

#[derive(Debug)]
pub enum PopoverOutput {
    Opened,
    Closed,
    Command(Command),
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = PopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "removable-popover",
            add_css_class: "popover-size-medium",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[name = "scroller"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                        set_vexpand: false,
                        set_propagate_natural_height: true,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 0,

                            #[name = "empty_label"]
                            gtk::Label {
                                add_css_class: "dim-label",
                                set_label: "No removable devices",
                                set_margin_top: 12,
                                set_margin_bottom: 12,
                                set_xalign: 0.0,
                                set_visible: true,
                            },

                            #[local_ref]
                            devices_widget -> gtk::Box {},
                        },
                    },
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
                header: Some("Removable devices".into()),
                items: Vec::new(),
            })
            .forward(sender.input_sender(), PopoverInput::DeviceCommand);
        let devices_widget = devices.widget().clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        popover_scroll::install_half_monitor_limit(&widgets.root, &widgets.scroller, &init.parent);
        devices.widget().set_visible(false);

        let opened_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            let _ = opened_sender.output(PopoverOutput::Opened);
        });

        let closed_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = closed_sender.output(PopoverOutput::Closed);
        });

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            devices,
            device_items: Vec::new(),
            empty_label: widgets.empty_label.clone(),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => {
                self.animation.toggle();
            }
            PopoverInput::UpdateState(state) => {
                let items = build_device_items(&state);
                self.empty_label.set_visible(items.is_empty());
                self.devices.widget().set_visible(!items.is_empty());
                if self.device_items != items {
                    self.devices.emit(DeviceListInput::Update(items.clone()));
                    self.device_items = items;
                }
            }
            PopoverInput::DeviceCommand(command) => {
                let _ = sender.output(PopoverOutput::Command(command));
            }
        }
    }
}

fn build_device_items(state: &State) -> Vec<DeviceListItem<Command>> {
    state
        .devices
        .iter()
        .map(|device| DeviceListItem {
            id: device.id.clone(),
            icon: device.icon.clone(),
            label: device.name.clone(),
            status: device_status(device),
            busy: device.busy,
            tooltip: Some(device_tooltip(device)),
            active: device.mounted_at.is_some(),
            visible: true,
            command: primary_device_command(device),
        })
        .collect()
}

fn primary_device_command(device: &StorageDevice) -> Option<Command> {
    if device.busy {
        return None;
    }

    if device.mounted_at.is_some() && device.can_unmount {
        Some(Command::Unmount {
            id: device.id.clone(),
        })
    } else if device.can_power_off {
        Some(Command::PowerOff {
            id: device.id.clone(),
        })
    } else if device.can_eject {
        Some(Command::Eject {
            id: device.id.clone(),
        })
    } else if device.can_mount {
        Some(Command::Mount {
            id: device.id.clone(),
        })
    } else {
        None
    }
}

fn device_status(device: &StorageDevice) -> String {
    if device.busy {
        "Working".into()
    } else if device.mounted_at.is_some() {
        "Mounted".into()
    } else if device.can_power_off {
        "Safe to remove".into()
    } else if device.can_eject {
        "Safe to remove".into()
    } else if device.can_mount {
        "Available".into()
    } else {
        "Not mounted".into()
    }
}

fn device_tooltip(device: &StorageDevice) -> String {
    let mut parts = Vec::new();
    if let Some(mounted_at) = &device.mounted_at {
        parts.push(format!("Mounted at {}", mounted_at.display()));
    }
    if let Some(size) = device.size_bytes {
        parts.push(format_size(size));
    }
    if let Some(filesystem) = &device.filesystem {
        parts.push(filesystem.clone());
    }

    if parts.is_empty() {
        device.name.clone()
    } else {
        parts.join(" - ")
    }
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1000.0 && unit + 1 < UNITS.len() {
        size /= 1000.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> StorageDevice {
        StorageDevice {
            id: "device".into(),
            name: "USB Drive".into(),
            icon: "drive-removable-media-symbolic".into(),
            can_mount: true,
            can_unmount: true,
            can_eject: true,
            can_power_off: true,
            ..StorageDevice::default()
        }
    }

    #[test]
    fn mounted_device_primary_action_is_unmount() {
        let device = StorageDevice {
            mounted_at: Some("/run/media/alex/USB".into()),
            ..device()
        };

        assert_eq!(
            primary_device_command(&device),
            Some(Command::Unmount {
                id: "device".into()
            })
        );
    }

    #[test]
    fn unmounted_device_prefers_safe_removal_then_mount() {
        let mut device = device();
        assert_eq!(
            primary_device_command(&device),
            Some(Command::PowerOff {
                id: "device".into()
            })
        );

        device.can_power_off = false;
        device.can_eject = false;
        assert_eq!(
            primary_device_command(&device),
            Some(Command::Mount {
                id: "device".into()
            })
        );
    }

    #[test]
    fn busy_device_has_no_primary_action() {
        let device = StorageDevice {
            busy: true,
            ..device()
        };

        assert_eq!(primary_device_command(&device), None);
    }

    #[test]
    fn device_status_uses_user_facing_states() {
        assert_eq!(device_status(&device()), "Safe to remove");

        let mounted = StorageDevice {
            mounted_at: Some("/run/media/alex/USB".into()),
            ..device()
        };
        assert_eq!(device_status(&mounted), "Mounted");

        let mountable = StorageDevice {
            can_power_off: false,
            can_eject: false,
            ..device()
        };
        assert_eq!(device_status(&mountable), "Available");
    }
}
