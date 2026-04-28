#![allow(unused_assignments)]

use std::cell::Cell;
use std::rc::Rc;

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use crate::{
    applets::network::components::{
        VpnSection, VpnSectionInput, WifiSection, WifiSectionInput, WiredSection, WiredSectionInput,
    },
    components::{hero::HeroView, popover_shell::PopoverShell},
    services::network::{Command, State},
};

use super::format;

pub struct Popover {
    popover: gtk::Popover,
    hero_icon_name: String,
    hero_subtitle: String,
    wifi_enabled: bool,
    wifi_toggle_sensitive: bool,
    updating_wifi: Rc<Cell<bool>>,
    wifi_section: Controller<WifiSection>,
    wired_section: Controller<WiredSection>,
    vpn_section: Controller<VpnSection>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    UpdateState(State),
    SetWifiEnabled(bool),
    SectionCommand(Command),
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
            add_css_class: "network-popover",
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
                    wifi_widget -> gtk::Box {},

                    #[local_ref]
                    wired_widget -> gtk::Box {},

                    #[local_ref]
                    vpn_widget -> gtk::Box {},
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let wifi_section = WifiSection::builder()
            .launch(())
            .forward(sender.input_sender(), PopoverInput::SectionCommand);
        let wifi_widget = wifi_section.widget().clone();

        let wired_section = WiredSection::builder().launch(()).detach();
        let wired_widget = wired_section.widget().clone();

        let vpn_section = VpnSection::builder()
            .launch(())
            .forward(sender.input_sender(), PopoverInput::SectionCommand);
        let vpn_widget = vpn_section.widget().clone();

        let updating_wifi = Rc::new(Cell::new(false));
        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let toggle_guard = updating_wifi.clone();
        let toggle_sender = sender.clone();
        widgets.hero.toggle.connect_state_set(move |_, active| {
            if toggle_guard.get() {
                return glib::Propagation::Stop;
            }

            toggle_sender.input(PopoverInput::SetWifiEnabled(active));
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
            hero_icon_name: "network-offline-symbolic".into(),
            hero_subtitle: "Not connected".into(),
            wifi_enabled: false,
            wifi_toggle_sensitive: false,
            updating_wifi,
            wifi_section,
            wired_section,
            vpn_section,
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
                self.hero_icon_name = icon_name_for_state(&state).into();
                self.hero_subtitle = format::hero_subtitle(&state);
                self.wifi_enabled = state.snapshot.status.wifi_enabled;
                self.wifi_toggle_sensitive =
                    state.snapshot.status.enabled && state.snapshot.status.wifi_hw_enabled;

                self.wifi_section.emit(WifiSectionInput::Update {
                    access_points: state.snapshot.wifi_access_points,
                    wifi_enabled: state.snapshot.status.wifi_enabled,
                    active_action: state.active_action.clone(),
                });
                self.wired_section
                    .emit(WiredSectionInput::Update(state.snapshot.devices));
                self.vpn_section
                    .emit(VpnSectionInput::Update(state.snapshot.saved_vpns));
            }
            PopoverInput::SetWifiEnabled(enabled) => {
                let _ = sender.output(PopoverOutput::Command(Command::SetWifiEnabled(enabled)));
            }
            PopoverInput::SectionCommand(command) => {
                let _ = sender.output(PopoverOutput::Command(command));
            }
        }
    }

    fn post_view() {
        hero.icon.set_icon_name(Some(&model.hero_icon_name));
        hero.title.set_label("Network");
        hero.subtitle.set_label(&model.hero_subtitle);
        hero.toggle.set_sensitive(model.wifi_toggle_sensitive);

        if hero.toggle.is_active() != model.wifi_enabled {
            model.updating_wifi.set(true);
            hero.toggle.set_active(model.wifi_enabled);
            hero.toggle.set_state(model.wifi_enabled);
            model.updating_wifi.set(false);
        }
    }
}

fn icon_name_for_state(state: &State) -> &str {
    if state.snapshot.status.icon.is_empty() {
        "network-offline-symbolic"
    } else {
        &state.snapshot.status.icon
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::network::{
        NetworkActiveAction, NetworkServiceHealth, NetworkSnapshot, NetworkStatus,
    };

    #[test]
    fn hero_subtitle_prefers_health_then_activity_then_status() {
        let mut state = State {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                status: NetworkStatus {
                    enabled: true,
                    wifi_enabled: true,
                    wifi_hw_enabled: true,
                    primary_connection: "Home".into(),
                    ..NetworkStatus::default()
                },
                ..NetworkSnapshot::default()
            },
            ..State::default()
        };

        assert_eq!(format::hero_subtitle(&state), "Connected to Home");

        state.active_action = Some(NetworkActiveAction::SetWifiEnabled(false));
        assert_eq!(format::hero_subtitle(&state), "Turning Wi-Fi off");

        state.active_action = None;
        state.scanning = true;
        assert_eq!(format::hero_subtitle(&state), "Scanning");

        state.health = NetworkServiceHealth::Reconnecting { attempt: 2 };
        assert_eq!(format::hero_subtitle(&state), "Reconnecting");
    }
}
