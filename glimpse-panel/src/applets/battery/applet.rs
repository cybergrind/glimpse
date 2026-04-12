use glimpse::providers::battery::{BatteryEvent, BatteryProvider, BatteryState, BatteryStatus};
use glimpse::providers::power::{PowerEvent, PowerProfiles, PowerProvider};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::config::BatteryConfig;
use super::popover::{
    BatteryPopover, BatteryPopoverInit, BatteryPopoverInput, BatteryPopoverOutput,
};

pub struct Battery {
    config: BatteryConfig,
    conn: zbus::Connection,
    icon_name: String,
    label: String,
    tooltip: String,
    battery_tooltip: String,
    degraded_tooltip: String,
    visible: bool,
    latest_status: Option<BatteryStatus>,
    latest_profiles: Option<PowerProfiles>,
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
    PopoverOutput(BatteryPopoverOutput),
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
        let provider_conn = conn.clone();
        let popover = BatteryPopover::builder()
            .launch(BatteryPopoverInit {
                parent: root.clone(),
                has_settings_command: has_settings_command(&init.config.settings_command),
            })
            .forward(sender.input_sender(), BatteryInput::PopoverOutput);

        let model = Battery {
            config: init.config,
            conn,
            icon_name: "battery-missing-symbolic".into(),
            label: String::new(),
            tooltip: String::new(),
            battery_tooltip: String::new(),
            degraded_tooltip: String::new(),
            visible: false,
            latest_status: None,
            latest_profiles: None,
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
                        let conn = provider_conn.clone();
                        async move {
                            let mut provider = BatteryProvider::new(conn);
                            if let Err(e) = provider.run(bat_tx, cancel).await {
                                tracing::error!("battery provider: {e}");
                            }
                        }
                    });

                    let (pwr_tx, mut pwr_rx) = mpsc::channel::<PowerEvent>(8);
                    tokio::spawn({
                        let cancel = cancel.clone();
                        let conn = provider_conn.clone();
                        async move {
                            let mut provider = PowerProvider::new(conn);
                            if let Err(e) = provider.run(pwr_tx, cancel).await {
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

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            BatteryInput::Update(status) => {
                tracing::debug!(pct = status.percentage, state = ?status.state, "battery applet: update");
                self.icon_name = status.icon_name.clone();
                self.visible = status.present;
                self.latest_status = Some(status.clone());

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
                self.battery_tooltip = format_template(tooltip_fmt, &vars);
                if self.degraded_tooltip.is_empty() {
                    self.tooltip = self.battery_tooltip.clone();
                }

                if should_sync_popover(self.popover.widget().is_visible()) {
                    self.popover.emit(BatteryPopoverInput::UpdateStatus(status));
                }
            }
            BatteryInput::UpdateProfiles(profiles) => {
                self.latest_profiles = Some(profiles.clone());
                self.degraded_tooltip = if profiles.performance_degraded.is_empty() {
                    root.remove_css_class("degraded");
                    String::new()
                } else {
                    root.add_css_class("degraded");
                    format!("Performance degraded: {}", profiles.performance_degraded)
                };
                self.tooltip = if self.degraded_tooltip.is_empty() {
                    self.battery_tooltip.clone()
                } else {
                    self.degraded_tooltip.clone()
                };
                if should_sync_popover(self.popover.widget().is_visible()) {
                    self.popover
                        .emit(BatteryPopoverInput::UpdateProfiles(profiles));
                }
            }
            BatteryInput::PopoverOutput(BatteryPopoverOutput::SetProfile(profile)) => {
                let conn = self.conn.clone();
                sender.command(move |_out, shutdown| {
                    shutdown
                        .register(async move {
                            if let Err(error) = PowerProvider::new(conn).set_profile(&profile).await
                            {
                                tracing::warn!(
                                    error = %error,
                                    "battery applet: failed to set power profile"
                                );
                            }
                        })
                        .drop_on_shutdown()
                });
            }
            BatteryInput::PopoverOutput(BatteryPopoverOutput::OpenSettings) => {
                spawn_settings_command(&self.config.settings_command);
            }
            BatteryInput::TogglePopover => {
                let was_visible = self.popover.widget().is_visible();
                self.popover.emit(BatteryPopoverInput::Toggle);
                if !was_visible {
                    if let Some(status) = self.latest_status.clone() {
                        self.popover.emit(BatteryPopoverInput::UpdateStatus(status));
                    }
                    if let Some(profiles) = self.latest_profiles.clone() {
                        self.popover.emit(BatteryPopoverInput::UpdateProfiles(profiles));
                    }
                }
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

fn spawn_settings_command(command: &str) {
    if !has_settings_command(command) {
        return;
    }

    let parts: Vec<&str> = command.split_whitespace().collect();
    if let Some((&prog, args)) = parts.split_first() {
        let _ = std::process::Command::new(prog).args(args).spawn();
    }
}

fn has_settings_command(command: &str) -> bool {
    !command.trim().is_empty()
}

fn should_sync_popover(is_visible: bool) -> bool {
    is_visible
}

#[cfg(test)]
mod tests {
    use super::{has_settings_command, should_sync_popover};

    #[test]
    fn settings_command_capability_trims_whitespace() {
        assert!(!has_settings_command(""));
        assert!(!has_settings_command("   \t"));
        assert!(has_settings_command("gnome-control-center power"));
    }

    #[test]
    fn hidden_popover_does_not_receive_updates() {
        assert!(!should_sync_popover(false));
        assert!(should_sync_popover(true));
    }
}
