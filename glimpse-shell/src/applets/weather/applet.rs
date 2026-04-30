use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        framework::ServiceCommand,
        weather::{
            WeatherHandle,
            model::{Command, Config as ServiceConfig, State},
        },
    },
};

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub city_name: String,
    pub geolocate: bool,
    pub hourly_slots: usize,
    pub forecast_days: usize,
    #[serde(alias = "label")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub refresh_interval: u64,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid weather applet config, using defaults");
                Self::default()
            }
        }
    }

    fn service_config(&self) -> ServiceConfig {
        ServiceConfig {
            city_name: self.city_name.clone(),
            hourly_slots: self.hourly_slots,
            forecast_days: self.forecast_days,
            refresh_interval: self.refresh_interval,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        let service = ServiceConfig::default();
        Self {
            city_name: service.city_name,
            geolocate: false,
            hourly_slots: service.hourly_slots,
            forecast_days: service.forecast_days,
            label_format: format::DEFAULT_LABEL_FORMAT.into(),
            tooltip_format: format::DEFAULT_TOOLTIP_FORMAT.into(),
            refresh_interval: service.refresh_interval,
        }
    }
}

pub struct Applet {
    config: Config,
    icon_name: String,
    label: String,
    tooltip: String,
    service: WeatherHandle,
    popover: Controller<Popover>,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: WeatherHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
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
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

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
            },

            gtk::Label {
                set_valign: gtk::Align::Center,
                #[watch]
                set_label: &model.label,
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
        let popover = Popover::builder()
            .launch(PopoverInit {
                parent: root.clone(),
            })
            .detach();

        let state = init.service.snapshot();
        let model = Applet {
            icon_name: format::icon_name(&state).into(),
            label: format::label(&init.config.label_format, &state),
            tooltip: format::tooltip(&init.config.tooltip_format, &state),
            config: init.config,
            service: init.service,
            popover,
            subscription_cancel: CancellationToken::new(),
        };

        let service = model.service.clone();
        let cancel = model.subscription_cancel.clone();
        let subscription_sender = sender.clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            subscription_sender.input(Input::ServiceStateChanged(sub.borrow().clone()));

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }

                        subscription_sender.input(Input::ServiceStateChanged(sub.borrow().clone()));
                    }
                }
            }
        });

        model.send_command(Command::Configure(model.config.service_config()));

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => self.apply_state(state),
            Input::Reconfigure(config) => {
                self.config = config;
                self.send_command(Command::Configure(self.config.service_config()));
                self.apply_state(self.service.snapshot());
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
        }
    }
}

impl Applet {
    fn apply_state(&mut self, state: State) {
        self.icon_name = format::icon_name(&state).into();
        self.label = format::label(&self.config.label_format, &state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
        self.popover.emit(PopoverInput::Update(state));
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send weather command");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_accepts_absent_and_empty_settings() {
        assert_eq!(Config::from_raw(&None), Config::default());
        assert_eq!(
            Config::from_raw(&Some(AppletConfig::default())),
            Config::default()
        );
    }

    #[test]
    fn config_parses_weather_settings() {
        let config = Config::from_raw(&Some(AppletConfig {
            settings: toml::toml! {
                city_name = "Warsaw, PL"
                geolocate = true
                hourly_slots = 4
                forecast_days = 3
                label = "{condition}"
                tooltip = "{location}"
                refresh_interval = 900
            }
            .into(),
            ..AppletConfig::default()
        }));

        assert_eq!(config.city_name, "Warsaw, PL");
        assert!(config.geolocate);
        assert_eq!(config.hourly_slots, 4);
        assert_eq!(config.forecast_days, 3);
        assert_eq!(config.label_format, "{condition}");
        assert_eq!(config.tooltip_format, "{location}");
        assert_eq!(config.refresh_interval, 900);
    }

    #[test]
    fn service_config_keeps_fetch_settings_only() {
        let config = Config {
            city_name: "Warsaw, PL".into(),
            geolocate: true,
            hourly_slots: 4,
            forecast_days: 3,
            refresh_interval: 900,
            ..Config::default()
        };

        let service = config.service_config();

        assert_eq!(service.city_name, "Warsaw, PL");
        assert_eq!(service.hourly_slots, 4);
        assert_eq!(service.forecast_days, 3);
        assert_eq!(service.refresh_interval, 900);
    }
}
