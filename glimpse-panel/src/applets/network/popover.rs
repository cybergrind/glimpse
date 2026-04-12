#![allow(unused_assignments)]

use glimpse::network::protocol::NetworkActiveAction;
use glimpse::providers::network::NetworkSnapshot;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::{
    NetworkAction, NetworkHero, NetworkHeroInput, VpnSection, VpnSectionInput, WifiSection,
    WifiSectionInput, WiredSection, WiredSectionInput,
};

pub struct NetworkPopover {
    popover: gtk::Popover,
    hero: Controller<NetworkHero>,
    wifi_section: Controller<WifiSection>,
    wired_section: Controller<WiredSection>,
    vpn_section: Controller<VpnSection>,
    show_settings_button: bool,
}

pub struct NetworkPopoverInit {
    pub parent: gtk::Box,
    pub show_settings_button: bool,
}

#[derive(Debug, Clone)]
pub enum NetworkPopoverInput {
    Toggle,
    Close,
    UpdateState {
        snapshot: NetworkSnapshot,
        active_action: Option<NetworkActiveAction>,
        scanning: bool,
    },
    ComponentAction(NetworkAction),
    OpenSettings,
}

#[derive(Debug, Clone)]
pub enum NetworkPopoverOutput {
    Opened,
    Closed,
    ToggleWifi(bool),
    ConnectWifi { ssid: String, path: String },
    ConnectSaved { uuid: String },
    Disconnect { uuid: String },
    Forget { uuid: String },
    OpenSettings,
}

#[relm4::component(pub)]
impl SimpleComponent for NetworkPopover {
    type Init = NetworkPopoverInit;
    type Input = NetworkPopoverInput;
    type Output = NetworkPopoverOutput;

    view! {
        root = gtk::Popover {
            set_hexpand: false,
            add_css_class: "network-popover",

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_overflow: gtk::Overflow::Hidden,

                #[local_ref]
                hero_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[local_ref]
                wifi_widget -> gtk::Box {},

                #[local_ref]
                wired_widget -> gtk::Box {},

                #[local_ref]
                vpn_widget -> gtk::Box {},

                gtk::Box {
                    #[watch]
                    set_visible: model.show_settings_button,
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },

                    gtk::Button {
                        add_css_class: "flat",
                        add_css_class: "settings-btn",
                        connect_clicked => NetworkPopoverInput::OpenSettings,

                        gtk::Label {
                            set_label: "Network Settings",
                            set_halign: gtk::Align::Start,
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
        let hero = NetworkHero::builder()
            .launch(())
            .forward(sender.input_sender(), NetworkPopoverInput::ComponentAction);
        let hero_widget = hero.widget().clone();

        let wifi_section = WifiSection::builder()
            .launch(())
            .forward(sender.input_sender(), NetworkPopoverInput::ComponentAction);
        let wifi_widget = wifi_section.widget().clone();

        let wired_section = WiredSection::builder().launch(()).detach();
        let wired_widget = wired_section.widget().clone();

        let vpn_section = VpnSection::builder()
            .launch(())
            .forward(sender.input_sender(), NetworkPopoverInput::ComponentAction);
        let vpn_widget = vpn_section.widget().clone();

        let mut model = NetworkPopover {
            popover: gtk::Popover::new(),
            hero,
            wifi_section,
            wired_section,
            vpn_section,
            show_settings_button: init.show_settings_button,
        };

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        widgets.root.add_css_class("network-popover");
        model.popover = widgets.root.clone();

        let show_sender = widgets.root.clone();
        let sender_clone = sender.clone();
        show_sender.connect_show(move |_| {
            let _ = sender_clone.output(NetworkPopoverOutput::Opened);
        });

        let close_sender = sender.clone();
        widgets.root.connect_closed(move |_| {
            let _ = close_sender.output(NetworkPopoverOutput::Closed);
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NetworkPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            NetworkPopoverInput::Close => self.popover.popdown(),
            NetworkPopoverInput::UpdateState {
                snapshot,
                active_action,
                scanning,
            } => {
                self.hero.emit(NetworkHeroInput::Update {
                    status: snapshot.status.clone(),
                    scanning,
                });
                self.wifi_section.emit(WifiSectionInput::Update {
                    access_points: snapshot.wifi_access_points,
                    wifi_enabled: snapshot.status.wifi_enabled,
                    active_action: active_action.clone(),
                });
                self.wired_section
                    .emit(WiredSectionInput::Update(snapshot.devices));
                self.vpn_section.emit(VpnSectionInput::Update {
                    vpns: snapshot.saved_vpns,
                    active_action,
                });
            }
            NetworkPopoverInput::ComponentAction(action) => {
                self.emit_action(action, sender);
            }
            NetworkPopoverInput::OpenSettings => {
                let _ = sender.output(NetworkPopoverOutput::OpenSettings);
            }
        }
    }
}

impl NetworkPopover {
    fn emit_action(&self, action: NetworkAction, sender: ComponentSender<Self>) {
        let output = match action {
            NetworkAction::ToggleWifi(enabled) => NetworkPopoverOutput::ToggleWifi(enabled),
            NetworkAction::ConnectWifi { ssid, path } => {
                NetworkPopoverOutput::ConnectWifi { ssid, path }
            }
            NetworkAction::ConnectSaved { uuid } => NetworkPopoverOutput::ConnectSaved { uuid },
            NetworkAction::Disconnect { uuid } => NetworkPopoverOutput::Disconnect { uuid },
            NetworkAction::Forget { uuid } => NetworkPopoverOutput::Forget { uuid },
        };
        let _ = sender.output(output);
    }
}
