use glimpse::providers::battery::{BatteryEvent, BatteryProvider, BatteryState, BatteryStatus};
use glimpse::providers::power::{PowerEvent, PowerProfiles, PowerProvider};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::config::BatteryConfig;
use super::popover::{BatteryPopover, BatteryPopoverInit, BatteryPopoverInput};

pub struct Battery {
    config: BatteryConfig,
    icon_name: String,
    label: String,
    tooltip: String,
    visible: bool,
    popover: Controller<BatteryPopover>,
}

pub struct BatteryInit {
    pub config: BatteryConfig,
    pub conn: zbus::Connection,
}

#[derive(Debug)]
pub enum BatteryInput {
    Update(BatteryStatus),
    UpdateProfiles(PowerProfiles),
    TogglePopover,
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Battery {
    type Init = BatteryInit;
    type Input = BatteryInput;
    type Output = ();
    type CommandOutput = BatteryInput;

    view! {
        gtk::Box {
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "battery",
            #[watch]
            set_visible: model.visible,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(BatteryInput::TogglePopover);
                }
            },

            gtk::Image {
                #[watch]
                set_icon_name: Some(&model.icon_name),
                set_pixel_size: 16,
                #[watch]
                set_visible: model.config.show_icon,
            },

            gtk::Label {
                #[watch]
                set_label: &model.label,
                add_css_class: "battery-label",
                #[watch]
                set_visible: !model.label.is_empty(),
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let conn = init.conn;
        let popover = BatteryPopover::builder()
            .launch(BatteryPopoverInit {
                parent: root.clone(),
                conn: conn.clone(),
                settings_command: init.config.settings_command.clone(),
            })
            .detach();

        let model = Battery {
            config: init.config,
            icon_name: "battery-missing-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            visible: false,
            popover,
        };

        sender.command(move |cmd_tx, shutdown| {
            shutdown
                .register(async move {
                    tracing::debug!("battery applet: starting providers");
                    let cancel = CancellationToken::new();

                    let (bat_tx, mut bat_rx) = mpsc::channel::<BatteryEvent>(8);
                    tokio::spawn({
                        let cancel = cancel.clone();
                        let conn = conn.clone();
                        async move {
                            let mut provider = BatteryProvider::new();
                            if let Err(e) = provider.run(conn, bat_tx, cancel).await {
                                tracing::error!("battery provider: {e}");
                            }
                        }
                    });

                    let (pwr_tx, mut pwr_rx) = mpsc::channel::<PowerEvent>(8);
                    tokio::spawn({
                        let cancel = cancel.clone();
                        async move {
                            let mut provider = PowerProvider::new();
                            if let Err(e) = provider.run(conn, pwr_tx, cancel).await {
                                tracing::error!("power provider: {e}");
                            }
                        }
                    });

                    loop {
                        tokio::select! {
                            event = bat_rx.recv() => match event {
                                Some(BatteryEvent::StatusChanged(status)) => {
                                    let _ = cmd_tx.send(BatteryInput::Update(status));
                                }
                                Some(BatteryEvent::DevicesChanged(_)) => {}
                                None => {
                                    cancel.cancel();
                                    let _ = cmd_tx.send(BatteryInput::Unavailable);
                                    break;
                                }
                            },
                            event = pwr_rx.recv() => match event {
                                Some(PowerEvent::ProfilesChanged(profiles)) => {
                                    let _ = cmd_tx.send(BatteryInput::UpdateProfiles(profiles));
                                }
                                Some(PowerEvent::ActionsChanged(_)) => {}
                                None => break,
                            },
                        }
                    }
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

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            BatteryInput::Update(status) => {
                tracing::info!(pct = status.percentage, state = ?status.state, "battery applet: update");
                self.icon_name = status.icon_name.clone();
                self.visible = status.present;

                let vars = FormatVars {
                    percentage: status.percentage,
                    state: &status.state,
                    energy_rate: status.energy_rate,
                    capacity: status.capacity,
                    time_to_empty: status.time_to_empty,
                    time_to_full: status.time_to_full,
                };

                let (label_fmt, tooltip_fmt) = if status.on_battery {
                    (
                        &self.config.label_on_battery,
                        &self.config.tooltip_on_battery,
                    )
                } else {
                    (&self.config.label_on_ac, &self.config.tooltip_on_ac)
                };

                self.label = format_template(label_fmt, &vars);
                self.tooltip = format_template(tooltip_fmt, &vars);

                self.popover.emit(BatteryPopoverInput::UpdateStatus(status));
            }
            BatteryInput::UpdateProfiles(profiles) => {
                self.popover
                    .emit(BatteryPopoverInput::UpdateProfiles(profiles));
            }
            BatteryInput::TogglePopover => {
                self.popover.emit(BatteryPopoverInput::Toggle);
            }
            BatteryInput::Unavailable => {
                tracing::warn!("battery applet: provider unavailable");
                self.visible = false;
            }
        }
    }
}

struct FormatVars<'a> {
    percentage: u8,
    state: &'a BatteryState,
    energy_rate: f64,
    capacity: f64,
    time_to_empty: i64,
    time_to_full: i64,
}

fn format_template(template: &str, vars: &FormatVars) -> String {
    if template.is_empty() {
        return String::new();
    }

    let time_left = match vars.state {
        BatteryState::Discharging if vars.time_to_empty > 0 => {
            format!("{} remaining", format_duration(vars.time_to_empty))
        }
        BatteryState::Charging if vars.time_to_full > 0 => {
            format!("{} until full", format_duration(vars.time_to_full))
        }
        BatteryState::FullyCharged => "fully charged".into(),
        _ => String::new(),
    };
    let power = if vars.energy_rate > 0.0 {
        format!("{:.1}W", vars.energy_rate)
    } else {
        String::new()
    };
    let state_str = match vars.state {
        BatteryState::Charging => "charging",
        BatteryState::Discharging => "discharging",
        BatteryState::Empty => "empty",
        BatteryState::FullyCharged => "fully-charged",
        BatteryState::PendingCharge => "pending-charge",
        BatteryState::PendingDischarge => "pending-discharge",
        BatteryState::Unknown => "unknown",
    };

    template
        .replace("{percentage}", &vars.percentage.to_string())
        .replace("{state}", state_str)
        .replace("{time_left}", &time_left)
        .replace("{power}", &power)
        .replace("{health}", &format!("{:.0}%", vars.capacity))
        .trim_end_matches([' ', ',', '-', '—'])
        .to_owned()
}

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else {
        format!("{minutes}m")
    }
}
