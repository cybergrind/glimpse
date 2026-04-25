use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::battery::{BatteryHandle, BatteryState, BatteryStatus, State},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    show_icon: bool,
    #[serde(alias = "label_format")]
    label_on_battery: String,
    #[serde(alias = "label_format_on_ac")]
    label_on_ac: String,
    #[serde(alias = "tooltip_format")]
    tooltip_on_battery: String,
    #[serde(alias = "tooltip_format_on_ac")]
    tooltip_on_ac: String,
    settings_command: String,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid battery applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_icon: true,
            label_on_battery: "{percentage}%".into(),
            label_on_ac: String::new(),
            tooltip_on_battery: "{percentage}% {state}, {time_left}".into(),
            tooltip_on_ac: "{percentage}% {state}".into(),
            settings_command: String::new(),
        }
    }
}

pub struct Applet {
    visible: bool,
    config: Config,
    tooltip: String,
    label: String,
    icon_name: String,
    service: BatteryHandle,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: BatteryHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    BatteryStateChanged(State),
    Reconfigure(Config),
    TogglePopover,
}

#[relm4::component(pub)]
impl SimpleComponent for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            add_css_class: "battery",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            #[watch]
            set_visible: model.visible,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() {None} else {Some(&model.tooltip)},

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
                },
            },

            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                #[watch]
                set_icon_name: Some(&model.icon_name),
                #[watch]
                set_visible: model.config.show_icon,
            },

            gtk::Label {
                set_valign: gtk::Align::Center,
                #[watch]
                set_label: &model.label,
                #[watch]
                set_visible: !model.label.is_empty(),
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Applet {
            label: String::new(),
            tooltip: String::new(),
            visible: false,
            config: init.config,
            icon_name: "battery-missing-symbolic".into(),
            service: init.service,
            subscription_cancel: CancellationToken::new(),
        };

        let service = model.service.clone();
        let cancel = model.subscription_cancel.clone();
        let subscription_sender = sender.clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            subscription_sender.input(Input::BatteryStateChanged(sub.borrow().clone()));

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }

                        subscription_sender.input(Input::BatteryStateChanged(sub.borrow().clone()));
                    }
                }
            }
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::BatteryStateChanged(state) => {
                apply_status(self, &state.status);
            }
            Input::Reconfigure(config) => {
                self.config = config;
                let snapshot = self.service.snapshot();
                apply_status(self, &snapshot.status);
            }
            Input::TogglePopover => {}
        }
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn apply_status(instance: &mut Applet, status: &BatteryStatus) {
    let (label_template, tooltip_template) = select_templates(&instance.config, status);

    instance.label = format_label(label_template, status);
    instance.tooltip = format_label(tooltip_template, status);
    instance.icon_name = status.icon_name.clone();
    instance.visible = status.present;
}

fn select_templates<'a>(config: &'a Config, status: &BatteryStatus) -> (&'a str, &'a str) {
    if status.on_battery {
        (&config.label_on_battery, &config.tooltip_on_battery)
    } else {
        (
            if config.label_on_ac.is_empty() {
                &config.label_on_battery
            } else {
                &config.label_on_ac
            },
            if config.tooltip_on_ac.is_empty() {
                &config.tooltip_on_battery
            } else {
                &config.tooltip_on_ac
            },
        )
    }
}

fn format_label(template: &str, status: &BatteryStatus) -> String {
    if template.is_empty() {
        return String::new();
    }

    template
        .replace("{percentage}", &status.percentage.to_string())
        .replace("{state}", format_state(&status.state).as_ref())
        .replace("{time_left}", &format_time_left(status))
        .trim_end_matches([' ', ',', '-', '—'])
        .to_owned()
}

fn format_state(state: &BatteryState) -> &'static str {
    match state {
        BatteryState::Charging => "charging",
        BatteryState::Discharging => "discharging",
        BatteryState::Empty => "empty",
        BatteryState::FullyCharged => "fully charged",
        BatteryState::PendingCharge => "pending charge",
        BatteryState::PendingDischarge => "pending discharge",
        BatteryState::Unknown => "unknown",
    }
}

fn format_time_left(status: &BatteryStatus) -> String {
    let seconds = if status.on_battery {
        status.time_to_empty
    } else {
        status.time_to_full
    };

    if seconds <= 0 {
        return String::new();
    }

    let minutes = seconds / 60;
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;

    if hours > 0 {
        format!("{hours}h {remaining_minutes}m")
    } else {
        format!("{remaining_minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(entries: impl IntoIterator<Item = (&'static str, toml::Value)>) -> Option<AppletConfig> {
        Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.into(), value))
                    .collect(),
            ),
        })
    }

    fn templates_for(config: &Config, on_battery: bool) -> (&str, &str) {
        let status = BatteryStatus {
            on_battery,
            ..BatteryStatus::default()
        };
        select_templates(config, &status)
    }

    #[test]
    fn config_from_raw_supports_legacy_label_aliases() {
        let config = Config::from_raw(&table([
            ("label_format", toml::Value::String("legacy".into())),
            (
                "tooltip_format",
                toml::Value::String("legacy tooltip".into()),
            ),
        ]));

        assert_eq!(templates_for(&config, true), ("legacy", "legacy tooltip"));
    }

    #[test]
    fn ac_templates_fall_back_to_battery_templates_when_empty() {
        let config = Config::from_raw(&table([(
            "label_format",
            toml::Value::String("generic".into()),
        )]));

        assert_eq!(templates_for(&config, false).0, "generic");
    }
}
