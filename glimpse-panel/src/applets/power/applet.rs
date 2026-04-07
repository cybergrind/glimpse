use std::sync::Arc;

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use tokio::sync::mpsc;

use super::dbus::{PowerAction, handle_action, monitor_battery, monitor_profiles};
use crate::applets::power::{
    PowerConfig,
    popover::{PowerPopover, PowerPopoverInit, PowerPopoverInput, PowerPopoverOutput},
};

struct PowerState {
    percentage: u8,
    charging: bool,
    icon_name: String,
    profiles: Vec<String>,
    active_profile: String,
    hidden: bool,
}

pub struct Power {
    config: PowerConfig,
    state: PowerState,
    action_tx: mpsc::Sender<PowerAction>,
    popover: Controller<PowerPopover>,
}

pub struct PowerInit {
    pub dbus: Arc<zbus::Connection>,
    pub config: PowerConfig,
}

#[derive(Debug)]
pub enum PowerInput {
    BatteryUpdate {
        percentage: u8,
        charging: bool,
        icon_name: String,
    },
    ProfilesUpdate {
        profiles: Vec<String>,
        active: String,
    },
    NoBattery,
    TogglePopover,
    ScrollProfile(f64),
    SetProfile(String),
    Suspend,
    Hibernate,
    Reboot,
    PowerOff,
}

#[derive(Debug)]
pub enum PowerCommand {
    BatteryUpdate {
        percentage: u8,
        charging: bool,
        icon_name: String,
    },
    ProfilesUpdate {
        profiles: Vec<String>,
        active: String,
    },
    NoBattery,
}

#[relm4::component(pub)]
impl Component for Power {
    type Init = PowerInit;
    type Input = PowerInput;
    type Output = ();
    type CommandOutput = PowerCommand;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "applet",
            add_css_class: "clickable",
            add_css_class: "power",

            #[watch]
            set_visible: !model.state.hidden,

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(PowerInput::TogglePopover);
                }
            },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.state.icon_name),
                set_pixel_size: 16,
            },
            gtk::Label {
                #[watch]
                set_label: &model
                    .config
                    .format
                    .replace("{}", &model.state.percentage.to_string()),
                #[watch]
                set_visible: model.config.percentage,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (action_tx, action_rx) = mpsc::channel::<PowerAction>(8);

        let popover = PowerPopover::builder()
            .launch(PowerPopoverInit {
                parent: root.clone(),
            })
            .forward(sender.input_sender(), |msg| match msg {
                PowerPopoverOutput::SetProfile(p) => PowerInput::SetProfile(p),
                PowerPopoverOutput::Suspend => PowerInput::Suspend,
                PowerPopoverOutput::Hibernate => PowerInput::Hibernate,
                PowerPopoverOutput::Reboot => PowerInput::Reboot,
                PowerPopoverOutput::PowerOff => PowerInput::PowerOff,
            });

        let model = Power {
            config: init.config,
            state: PowerState {
                percentage: 100,
                charging: false,
                icon_name: "battery-full-symbolic".to_string(),
                profiles: vec![],
                active_profile: String::new(),
                hidden: false,
            },
            action_tx,
            popover,
        };
        let widgets = view_output!();

        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let scroll_sender = sender.clone();
        scroll.connect_scroll(move |_, _dx, dy| {
            scroll_sender.input(PowerInput::ScrollProfile(dy));
            gtk::glib::Propagation::Stop
        });
        root.add_controller(scroll);

        sender.command(move |cmd_tx, shutdown| {
            shutdown
                .register(async move {
                    let mut action_rx = action_rx;
                    let sys_conn = match zbus::Connection::system().await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!("system bus connection failed: {e}");
                            return;
                        }
                    };

                    tracing::info!("power applet: starting monitors");
                    let (inner_tx, mut inner_rx) = mpsc::channel::<PowerCommand>(16);
                    tokio::spawn(monitor_battery(inner_tx.clone()));
                    tokio::spawn(monitor_profiles(inner_tx));

                    loop {
                        tokio::select! {
                            Some(pcmd) = inner_rx.recv() => { cmd_tx.send(pcmd).ok(); }
                            Some(action) = action_rx.recv() => {
                                handle_action(&sys_conn, action).await;
                            }
                            else => break,
                        }
                    }
                })
                .drop_on_shutdown()
        });

        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            PowerCommand::BatteryUpdate {
                percentage,
                charging,
                icon_name,
            } => sender.input(PowerInput::BatteryUpdate {
                percentage,
                charging,
                icon_name,
            }),
            PowerCommand::ProfilesUpdate { profiles, active } => {
                sender.input(PowerInput::ProfilesUpdate { profiles, active })
            }
            PowerCommand::NoBattery => sender.input(PowerInput::NoBattery),
        }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            PowerInput::BatteryUpdate {
                percentage,
                charging,
                icon_name,
            } => {
                tracing::info!(percentage, charging, icon = %icon_name, "power applet: battery update");
                self.state.percentage = percentage;
                self.state.charging = charging;
                self.state.icon_name = icon_name;
                if percentage <= self.config.low_battery_treshold && !charging {
                    root.add_css_class("low");
                } else {
                    root.remove_css_class("low");
                }
            }
            PowerInput::ProfilesUpdate { profiles, active } => {
                tracing::info!(active = %active, profiles = ?profiles, "power applet: profiles update");
                self.state.profiles = profiles.clone();
                self.state.active_profile = active.clone();
                self.popover
                    .emit(PowerPopoverInput::Update { profiles, active });
            }
            PowerInput::NoBattery => {
                self.state.hidden = self.config.hide_on_no_battery;
                tracing::info!("no battery detected");
            }
            PowerInput::TogglePopover => {
                self.popover.emit(PowerPopoverInput::Toggle);
            }
            PowerInput::ScrollProfile(dy) => {
                if self.state.profiles.is_empty() {
                    return;
                }
                let idx = self
                    .state
                    .profiles
                    .iter()
                    .position(|p| p == &self.state.active_profile)
                    .unwrap_or(0);
                let next = if dy < 0.0 {
                    idx.saturating_sub(1)
                } else {
                    (idx + 1).min(self.state.profiles.len() - 1)
                };
                sender.input(PowerInput::SetProfile(self.state.profiles[next].clone()));
            }
            PowerInput::SetProfile(profile) => {
                tracing::info!(profile = %profile, "power applet: set profile");
                self.state.active_profile = profile.clone();
                self.popover.emit(PowerPopoverInput::Update {
                    profiles: self.state.profiles.clone(),
                    active: profile.clone(),
                });
                self.action_tx
                    .try_send(PowerAction::SetProfile(profile))
                    .ok();
            }
            PowerInput::Suspend => {
                self.action_tx.try_send(PowerAction::Suspend).ok();
            }
            PowerInput::Hibernate => {
                self.action_tx.try_send(PowerAction::Hibernate).ok();
            }
            PowerInput::Reboot => {
                self.action_tx.try_send(PowerAction::Reboot).ok();
            }
            PowerInput::PowerOff => {
                self.action_tx.try_send(PowerAction::PowerOff).ok();
            }
        }
    }
}
