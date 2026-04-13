#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub use super::components::{
    BluetoothDeviceAction, BluetoothDeviceList, BluetoothDeviceListInput,
    BluetoothDeviceListOutput, BluetoothHero, BluetoothHeroInput, BluetoothHeroOutput, BtDevice,
};

pub struct BluetoothPopover {
    popover: gtk::Popover,
    hero: relm4::Controller<BluetoothHero>,
    device_list: relm4::Controller<BluetoothDeviceList>,
    show_settings_button: bool,
    powered: bool,
    discovering: bool,
    connected_count: u32,
    activity: Option<String>,
}

pub struct BluetoothPopoverInit {
    pub parent: gtk::Box,
    pub show_settings_button: bool,
}

#[derive(Debug)]
pub enum BluetoothPopoverInput {
    Toggle,
    Close,
    SetShowSettingsButton(bool),
    UpdateStatus { powered: bool, discovering: bool },
    UpdateDevices(Vec<BtDevice>),
    FinishDeviceAction { address: String },
    SetActivity(Option<String>),
    HeroOutput(BluetoothHeroOutput),
    DeviceListOutput(BluetoothDeviceListOutput),
    OpenSettings,
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
    OpenSettings,
}

#[relm4::component(pub)]
impl SimpleComponent for BluetoothPopover {
    type Init = BluetoothPopoverInit;
    type Input = BluetoothPopoverInput;
    type Output = BluetoothPopoverOutput;

    view! {
        root = gtk::Popover {
            add_css_class: "bluetooth-popover",
            set_hexpand: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "bluetooth-popover-body",

                #[local_ref]
                hero_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[local_ref]
                device_list_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                gtk::Box {
                    #[watch]
                    set_visible: model.show_settings_button,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Button {
                        add_css_class: "flat",
                        add_css_class: "settings-btn",
                        connect_clicked => BluetoothPopoverInput::OpenSettings,

                        gtk::Label {
                            set_label: "Bluetooth Settings",
                            set_halign: gtk::Align::Start,
                        },
                    },
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);

        let hero = BluetoothHero::builder()
            .launch(())
            .forward(sender.input_sender(), BluetoothPopoverInput::HeroOutput);
        let device_list = BluetoothDeviceList::builder().launch(()).forward(
            sender.input_sender(),
            BluetoothPopoverInput::DeviceListOutput,
        );

        let hero_widget = hero.widget().clone();
        let device_list_widget = device_list.widget().clone();
        let mut model = BluetoothPopover {
            popover: gtk::Popover::new(),
            hero,
            device_list,
            show_settings_button: init.show_settings_button,
            powered: false,
            discovering: false,
            connected_count: 0,
            activity: None,
        };
        let widgets = view_output!();
        model.popover = widgets.root.clone();
        model.sync_hero();

        let show_sender = sender.clone();
        widgets.root.connect_show(move |_| {
            tracing::info!("bluetooth popover: opened");
            let _ = show_sender.output(BluetoothPopoverOutput::Opened);
        });

        let close_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            tracing::info!("bluetooth popover: closed");
            let _ = close_sender.output(BluetoothPopoverOutput::Closed);
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BluetoothPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            BluetoothPopoverInput::Close => self.popover.popdown(),
            BluetoothPopoverInput::SetShowSettingsButton(show_settings_button) => {
                self.show_settings_button = show_settings_button;
            }
            BluetoothPopoverInput::UpdateStatus {
                powered,
                discovering,
            } => {
                self.powered = powered;
                self.discovering = discovering;
                self.sync_hero();
            }
            BluetoothPopoverInput::UpdateDevices(devices) => {
                self.device_list
                    .emit(BluetoothDeviceListInput::UpdateDevices(devices));
            }
            BluetoothPopoverInput::FinishDeviceAction { address } => {
                self.device_list
                    .emit(BluetoothDeviceListInput::FinishDeviceAction { address });
            }
            BluetoothPopoverInput::SetActivity(activity) => {
                self.activity = activity;
                self.sync_hero();
            }
            BluetoothPopoverInput::HeroOutput(output) => match output {
                BluetoothHeroOutput::SetPowered(powered) => {
                    let _ = sender.output(BluetoothPopoverOutput::SetPowered(powered));
                }
            },
            BluetoothPopoverInput::DeviceListOutput(output) => match output {
                BluetoothDeviceListOutput::ConnectedCount(count) => {
                    self.connected_count = count;
                    self.sync_hero();
                }
                BluetoothDeviceListOutput::DeviceAction {
                    address,
                    name,
                    action,
                } => {
                    let _ = sender.output(BluetoothPopoverOutput::DeviceAction {
                        address,
                        name,
                        action,
                    });
                }
            },
            BluetoothPopoverInput::OpenSettings => {
                let _ = sender.output(BluetoothPopoverOutput::OpenSettings);
            }
        }
    }
}

impl BluetoothPopover {
    fn sync_hero(&self) {
        self.hero.emit(BluetoothHeroInput::Update {
            powered: self.powered,
            discovering: self.discovering,
            connected_count: self.connected_count,
            activity: self.activity.clone(),
        });
    }
}
