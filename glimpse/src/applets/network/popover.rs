#![allow(unused_assignments)]

use glimpse::network::protocol::NetworkActiveAction;
use glimpse::network::provider::NetworkSnapshot;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::{
    NetworkAction, NetworkHero, NetworkHeroInput, VpnSection, VpnSectionInput, WifiSection,
    WifiSectionInput, WiredSection, WiredSectionInput,
};
use crate::components::{
    footer_action::{FooterAction, FooterActionInit},
    popover_shell::{PopoverShell, PopoverShellInit, PopoverShellInput},
};

pub struct NetworkPopover {
    popover: gtk::Popover,
    shell: Controller<PopoverShell>,
    hero: Controller<NetworkHero>,
    wifi_section: Controller<WifiSection>,
    wired_section: Controller<WiredSection>,
    vpn_section: Controller<VpnSection>,
    footer: Controller<FooterAction>,
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
    SetShowSettingsButton(bool),
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

            #[local_ref]
            shell_widget -> gtk::Box {},
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let shell = PopoverShell::builder()
            .launch(PopoverShellInit {
                show_footer: init.show_settings_button,
            })
            .detach();
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
        let footer = FooterAction::builder()
            .launch(FooterActionInit {
                title: "Network Settings".into(),
                subtitle: String::new(),
            })
            .detach();

        let mut model = NetworkPopover {
            popover: gtk::Popover::new(),
            shell,
            hero,
            wifi_section,
            wired_section,
            vpn_section,
            footer,
            show_settings_button: init.show_settings_button,
        };

        let shell_widget = model.shell.widget().clone();
        let shell_content = shell_widget
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should expose content box");
        let shell_footer = shell_content
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should expose footer box");
        shell_content.append(&hero_widget);
        shell_content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        shell_content.append(&wifi_widget);
        shell_content.append(&wired_widget);
        shell_content.append(&vpn_widget);
        shell_footer.append(model.footer.widget());

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        widgets.root.add_css_class("network-popover");
        model.popover = widgets.root.clone();

        let footer_button = model
            .footer
            .widget()
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("footer action should expose row root")
            .first_child()
            .and_downcast::<gtk::Button>()
            .expect("footer action row should expose button");
        let footer_sender = sender.clone();
        footer_button.connect_clicked(move |_| {
            footer_sender.input(NetworkPopoverInput::OpenSettings);
        });

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
            NetworkPopoverInput::SetShowSettingsButton(show_settings_button) => {
                self.show_settings_button = show_settings_button;
                self.shell
                    .emit(PopoverShellInput::SetFooterVisible(show_settings_button));
            }
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
