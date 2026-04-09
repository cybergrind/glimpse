use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::{
    BluetoothCommand, BluetoothCommandSender, device_list::DeviceList, hero::BluetoothHero,
};

pub use super::components::{BluetoothDeviceAction, BtDevice};

pub struct BluetoothPopover {
    popover: gtk::Popover,
    hero: BluetoothHero,
    device_list: DeviceList,
}

pub struct BluetoothPopoverInit {
    pub parent: gtk::Box,
    pub settings_command: String,
}

#[derive(Debug)]
pub enum BluetoothPopoverInput {
    Toggle,
    UpdateStatus { powered: bool, discovering: bool },
    UpdateDevices(Vec<BtDevice>),
    FinishDeviceAction { address: String },
    SetActivity(Option<String>),
}

#[derive(Debug, Clone)]
pub enum BluetoothPopoverOutput {
    Opened,
    Closed,
    SetPowered(bool),
    DeviceAction {
        address: String,
        name: String,
        action: BluetoothDeviceAction,
    },
}

impl SimpleComponent for BluetoothPopover {
    type Init = BluetoothPopoverInit;
    type Input = BluetoothPopoverInput;
    type Output = BluetoothPopoverOutput;
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Popover::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("bluetooth-popover");

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.set_hexpand(false);
        body.set_overflow(gtk::Overflow::Hidden);

        let output = sender.clone();
        let on_command: BluetoothCommandSender = Rc::new(move |command| match command {
            BluetoothCommand::SetPowered(powered) => {
                tracing::info!(powered, "bluetooth popover: power toggle requested");
                let _ = output.output(BluetoothPopoverOutput::SetPowered(powered));
            }
            BluetoothCommand::DeviceAction {
                address,
                name,
                action,
            } => {
                tracing::info!(?action, address = %address, name = %name, "bluetooth popover: device action requested");
                let _ = output.output(BluetoothPopoverOutput::DeviceAction {
                    address,
                    name,
                    action,
                });
            }
        });

        let (hero, hero_widget) = BluetoothHero::new(on_command.clone());
        body.append(&hero_widget);
        body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let (device_list, device_list_widget) = DeviceList::new(on_command);
        body.append(&device_list_widget);

        body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        if !init.settings_command.is_empty() {
            let cmd = init.settings_command;
            let lbl = gtk::Label::new(Some("Bluetooth Settings"));
            lbl.set_halign(gtk::Align::Start);
            let btn = gtk::Button::new();
            btn.set_child(Some(&lbl));
            btn.add_css_class("flat");
            btn.add_css_class("settings-btn");
            btn.connect_clicked(move |_| {
                if let Ok(mut child) = std::process::Command::new("sh").arg("-c").arg(&cmd).spawn() {
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                }
            });
            body.append(&btn);
        }

        let show_sender = sender.clone();
        root.connect_show(move |_| {
            tracing::info!("bluetooth popover: opened");
            let _ = show_sender.output(BluetoothPopoverOutput::Opened);
        });

        let close_sender = sender.clone();
        root.connect_closed(move |_| {
            tracing::info!("bluetooth popover: closed");
            let _ = close_sender.output(BluetoothPopoverOutput::Closed);
        });

        root.set_child(Some(&body));

        let model = BluetoothPopover {
            popover: root.clone(),
            hero,
            device_list,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BluetoothPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            BluetoothPopoverInput::UpdateStatus {
                powered,
                discovering,
            } => {
                self.hero.update_status(powered, discovering);
            }
            BluetoothPopoverInput::UpdateDevices(devices) => {
                let connected_count = self.device_list.update(devices);
                self.hero.update_connected_count(connected_count);
            }
            BluetoothPopoverInput::FinishDeviceAction { address } => {
                self.device_list.finish_action(&address);
            }
            BluetoothPopoverInput::SetActivity(activity) => {
                self.hero.set_activity(activity);
            }
        }
    }
}
