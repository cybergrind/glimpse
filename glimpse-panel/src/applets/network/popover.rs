use std::rc::Rc;

use glimpse::network::protocol::NetworkActiveAction;
use glimpse::providers::network::NetworkSnapshot;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::{
    NetworkCommand, NetworkCommandSender, NetworkHero, VpnSection, WifiSection, WiredSection,
};

pub struct NetworkPopover {
    popover: gtk::Popover,
    hero: NetworkHero,
    wifi_section: WifiSection,
    wired_section: WiredSection,
    vpn_section: VpnSection,
}

pub struct NetworkPopoverInit {
    pub parent: gtk::Box,
    pub show_settings_button: bool,
}

#[derive(Debug, Clone)]
pub enum NetworkPopoverInput {
    Toggle,
    UpdateState {
        snapshot: NetworkSnapshot,
        active_action: Option<NetworkActiveAction>,
        scanning: bool,
    },
}

#[derive(Debug, Clone)]
pub enum NetworkPopoverOutput {
    Opened,
    Closed,
    ToggleWifi(bool),
    ConnectWifi { ssid: String },
    ConnectSaved { uuid: String },
    Disconnect { uuid: String },
    Forget { uuid: String },
    OpenSettings,
}

impl SimpleComponent for NetworkPopover {
    type Init = NetworkPopoverInit;
    type Input = NetworkPopoverInput;
    type Output = NetworkPopoverOutput;
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
        root.add_css_class("network-popover");

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.set_hexpand(false);
        body.set_overflow(gtk::Overflow::Hidden);

        let output_sender = sender.clone();
        let on_command: NetworkCommandSender = Rc::new(move |command| {
            let output = match command {
                NetworkCommand::ToggleWifi(enabled) => NetworkPopoverOutput::ToggleWifi(enabled),
                NetworkCommand::ConnectWifi { ssid } => NetworkPopoverOutput::ConnectWifi { ssid },
                NetworkCommand::ConnectSaved { uuid } => {
                    NetworkPopoverOutput::ConnectSaved { uuid }
                }
                NetworkCommand::Disconnect { uuid } => NetworkPopoverOutput::Disconnect { uuid },
                NetworkCommand::Forget { uuid } => NetworkPopoverOutput::Forget { uuid },
                NetworkCommand::OpenSettings => NetworkPopoverOutput::OpenSettings,
            };
            let _ = output_sender.output(output);
        });

        let (hero, hero_widget) = NetworkHero::new(on_command.clone());
        body.append(&hero_widget);
        body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let (wifi_section, wifi_widget) = WifiSection::new(on_command.clone());
        body.append(&wifi_widget);

        let (wired_section, wired_widget) = WiredSection::new();
        body.append(&wired_widget);

        let (vpn_section, vpn_widget) = VpnSection::new(on_command.clone());
        body.append(&vpn_widget);

        if init.show_settings_button {
            body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

            let label = gtk::Label::new(Some("Network Settings"));
            label.set_halign(gtk::Align::Start);
            let button = gtk::Button::new();
            button.set_child(Some(&label));
            button.add_css_class("flat");
            button.add_css_class("settings-btn");
            let on_command = on_command.clone();
            button.connect_clicked(move |_| on_command(NetworkCommand::OpenSettings));
            body.append(&button);
        }

        let show_sender = sender.clone();
        root.connect_show(move |_| {
            let _ = show_sender.output(NetworkPopoverOutput::Opened);
        });

        let close_sender = sender.clone();
        root.connect_closed(move |_| {
            let _ = close_sender.output(NetworkPopoverOutput::Closed);
        });

        root.set_child(Some(&body));

        let model = NetworkPopover {
            popover: root.clone(),
            hero,
            wifi_section,
            wired_section,
            vpn_section,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NetworkPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            NetworkPopoverInput::UpdateState {
                snapshot,
                active_action,
                scanning,
            } => {
                self.hero.update(&snapshot.status, scanning);
                self.wifi_section.update(
                    &snapshot.wifi_access_points,
                    snapshot.status.wifi_enabled,
                    active_action.as_ref(),
                );
                self.wired_section.update(&snapshot.devices);
                self.vpn_section
                    .update(&snapshot.saved_vpns, active_action.as_ref());
            }
        }
    }
}
