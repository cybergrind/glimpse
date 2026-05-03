use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        bluetooth::{BluetoothHandle, Command, State},
        framework::ServiceCommand,
    },
};

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "label")]
    label_format: String,
    #[serde(alias = "tooltip")]
    tooltip_format: String,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid bluetooth applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            label_format: format::DEFAULT_LABEL_FORMAT.into(),
            tooltip_format: format::DEFAULT_TOOLTIP_FORMAT.into(),
        }
    }
}

pub struct Applet {
    config: Config,
    icon_name: String,
    label: String,
    tooltip: String,
    state: State,
    service: BluetoothHandle,
    popover: Controller<Popover>,
    popover_open: bool,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: BluetoothHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    TogglePopover,
    PopoverOutput(PopoverOutput),
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
            }
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
            .forward(sender.input_sender(), Input::PopoverOutput);

        let state = init.service.snapshot();
        let config = init.config;
        let model = Applet {
            icon_name: icon_name_for_state(&state).into(),
            label: format::label(&config.label_format, &state),
            tooltip: format::tooltip(&config.tooltip_format, &state),
            config,
            state,
            service: init.service,
            popover,
            popover_open: false,
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

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                self.icon_name = icon_name_for_state(&state).into();
                self.label = format::label(&self.config.label_format, &state);
                self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
                self.state = state.clone();
                if self.popover_open {
                    self.popover.emit(PopoverInput::UpdateState(state));
                }
            }
            Input::Reconfigure(config) => {
                self.config = config;
                let state = self.service.snapshot();
                self.label = format::label(&self.config.label_format, &state);
                self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
                self.state = state;
                if self.popover_open {
                    self.sync_popover();
                }
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverOutput(PopoverOutput::Opened) => {
                self.popover_open = true;
                self.sync_popover();
                self.send_command(Command::StartDiscovery);
            }
            Input::PopoverOutput(PopoverOutput::Closed) => {
                self.popover_open = false;
                self.send_command(Command::StopDiscovery);
            }
            Input::PopoverOutput(PopoverOutput::Command(command)) => {
                self.send_command(command);
            }
        }
    }
}

impl Applet {
    fn sync_popover(&self) {
        self.popover
            .emit(PopoverInput::UpdateState(self.state.clone()));
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send bluetooth command");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn icon_name_for_state(state: &State) -> &'static str {
    if !state.snapshot.status.powered {
        "bluetooth-disabled-symbolic"
    } else if state.snapshot.status.connected_count > 0 {
        "bluetooth-active-symbolic"
    } else {
        "bluetooth-symbolic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::bluetooth::{BluetoothSnapshot, BluetoothStatus};
    use toml::map::Map;

    #[test]
    fn applet_icon_reflects_power_and_connection_state() {
        let mut state = State::default();
        assert_eq!(icon_name_for_state(&state), "bluetooth-disabled-symbolic");

        state.snapshot = BluetoothSnapshot {
            status: BluetoothStatus {
                powered: true,
                connected_count: 0,
                discovering: false,
            },
            ..BluetoothSnapshot::default()
        };
        assert_eq!(icon_name_for_state(&state), "bluetooth-symbolic");

        state.snapshot.status.connected_count = 1;
        assert_eq!(icon_name_for_state(&state), "bluetooth-active-symbolic");
    }

    #[test]
    fn config_accepts_absent_and_empty_settings() {
        assert_eq!(Config::from_raw(&None), Config::default());
        assert_eq!(
            Config::from_raw(&Some(AppletConfig::default())),
            Config::default()
        );
    }

    #[test]
    fn config_rejects_unknown_settings_fields() {
        let config = AppletConfig {
            extends: None,
            settings: toml::Value::Table(Map::from_iter([(
                "settings_command".into(),
                toml::Value::String("blueman-manager".into()),
            )])),
        };

        assert_eq!(Config::from_raw(&Some(config)), Config::default());
    }

    #[test]
    fn config_accepts_label_and_tooltip_templates() {
        let config = AppletConfig {
            extends: None,
            settings: toml::Value::Table(Map::from_iter([
                (
                    "label_format".into(),
                    toml::Value::String("{devices}".into()),
                ),
                (
                    "tooltip_format".into(),
                    toml::Value::String("{devices} connected devices".into()),
                ),
            ])),
        };

        let parsed = Config::from_raw(&Some(config));

        assert_eq!(parsed.label_format, "{devices}");
        assert_eq!(parsed.tooltip_format, "{devices} connected devices");
    }
}
