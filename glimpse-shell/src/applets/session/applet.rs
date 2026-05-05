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
        session::{Command, SessionAction, SessionHandle, State},
    },
};

use super::{
    dialogs, format,
    popover::{Init as PopoverInit, Input as PopoverInput, Output as PopoverOutput, Popover},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "label")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub show_lock: bool,
    pub show_logout: bool,
    pub show_suspend: bool,
    pub show_hibernate: bool,
    pub show_reboot: bool,
    pub show_shutdown: bool,
    pub confirm_logout: bool,
    pub confirm_suspend: bool,
    pub confirm_hibernate: bool,
    pub confirm_reboot: bool,
    pub confirm_shutdown: bool,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid session applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            label_format: super::format::DEFAULT_LABEL_FORMAT.into(),
            tooltip_format: super::format::DEFAULT_TOOLTIP_FORMAT.into(),
            show_lock: true,
            show_logout: true,
            show_suspend: true,
            show_hibernate: false,
            show_reboot: true,
            show_shutdown: true,
            confirm_logout: true,
            confirm_suspend: true,
            confirm_hibernate: true,
            confirm_reboot: true,
            confirm_shutdown: true,
        }
    }
}

pub struct Applet {
    config: Config,
    root: gtk::Box,
    icon_name: &'static str,
    label: String,
    tooltip: String,
    state: State,
    service: SessionHandle,
    popover: Controller<Popover>,
    popover_open: bool,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: SessionHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    TogglePopover,
    PopoverOutput(PopoverOutput),
    Confirmed(SessionAction),
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
                set_icon_name: Some(model.icon_name),
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
                config: init.config.clone(),
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let state = init.service.snapshot();
        let config = init.config;
        let model = Applet {
            root: root.clone(),
            icon_name: icon_name_for_state(&state),
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
        let subscription_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            if subscription_sender
                .send(Input::ServiceStateChanged(sub.borrow().clone()))
                .is_err()
            {
                return;
            }

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }

                        if subscription_sender
                            .send(Input::ServiceStateChanged(sub.borrow().clone()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                self.icon_name = icon_name_for_state(&state);
                self.label = format::label(&self.config.label_format, &state);
                self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
                self.state = state.clone();
                if self.popover_open {
                    self.popover.emit(PopoverInput::UpdateState(state));
                }
            }
            Input::Reconfigure(config) => {
                self.config = config.clone();
                let state = self.service.snapshot();
                self.icon_name = icon_name_for_state(&state);
                self.label = format::label(&self.config.label_format, &state);
                self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
                self.state = state;
                if self.popover_open {
                    self.popover.emit(PopoverInput::Reconfigure(config));
                    self.sync_popover();
                }
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverOutput(PopoverOutput::Opened) => {
                self.popover_open = true;
                self.popover
                    .emit(PopoverInput::Reconfigure(self.config.clone()));
                self.sync_popover();
                self.send_command(Command::Refresh);
            }
            Input::PopoverOutput(PopoverOutput::Closed) => {
                self.popover_open = false;
            }
            Input::PopoverOutput(PopoverOutput::ActionRequested(action)) => {
                self.popover.emit(PopoverInput::Close);
                if let Some(spec) = dialogs::confirmation_spec(action, &self.config) {
                    let sender = sender.clone();
                    dialogs::show_confirmation(&self.root, spec, move || {
                        sender.input(Input::Confirmed(action));
                    });
                } else {
                    self.send_action(action);
                }
            }
            Input::Confirmed(action) => {
                self.send_action(action);
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
                tracing::warn!(%error, "failed to send session command");
            }
        });
    }

    fn send_action(&self, action: SessionAction) {
        self.send_command(Command::Run(action));
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn icon_name_for_state(state: &State) -> &'static str {
    match state.active_action {
        Some(SessionAction::Lock) => "system-lock-screen-symbolic",
        Some(SessionAction::Logout) => "system-log-out-symbolic",
        Some(SessionAction::Suspend) => "media-playback-pause-symbolic",
        Some(SessionAction::Hibernate) => "document-save-symbolic",
        Some(SessionAction::Reboot) => "system-reboot-symbolic",
        Some(SessionAction::PowerOff) => "system-shutdown-symbolic",
        None => "avatar-default-symbolic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::session::{SessionAction, State};
    use toml::map::Map;

    #[test]
    fn default_config_matches_session_applet_behavior() {
        let config = Config::default();

        assert_eq!(config.label_format, "{user}");
        assert_eq!(config.tooltip_format, "{user} on {host}");
        assert!(config.show_lock);
        assert!(config.show_logout);
        assert!(config.show_suspend);
        assert!(!config.show_hibernate);
        assert!(config.show_reboot);
        assert!(config.show_shutdown);
        assert!(config.confirm_logout);
        assert!(config.confirm_suspend);
        assert!(config.confirm_hibernate);
        assert!(config.confirm_reboot);
        assert!(config.confirm_shutdown);
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
    fn config_parses_session_settings() {
        let config = Config::from_raw(&Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(Map::from_iter([
                ("label".into(), toml::Value::String("{state}".into())),
                ("tooltip".into(), toml::Value::String("{user}".into())),
                ("show_hibernate".into(), toml::Value::Boolean(true)),
                ("confirm_suspend".into(), toml::Value::Boolean(false)),
            ])),
        }));

        assert_eq!(config.label_format, "{state}");
        assert_eq!(config.tooltip_format, "{user}");
        assert!(config.show_hibernate);
        assert!(!config.confirm_suspend);
    }

    #[test]
    fn config_rejects_unknown_settings_fields() {
        let config = Config::from_raw(&Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(Map::from_iter([(
                "settings_command".into(),
                toml::Value::String("unused".into()),
            )])),
        }));

        assert_eq!(config, Config::default());
    }

    #[test]
    fn icon_reflects_active_session_action() {
        let mut state = State::default();
        assert_eq!(icon_name_for_state(&state), "avatar-default-symbolic");

        state.active_action = Some(SessionAction::Lock);
        assert_eq!(icon_name_for_state(&state), "system-lock-screen-symbolic");

        state.active_action = Some(SessionAction::PowerOff);
        assert_eq!(icon_name_for_state(&state), "system-shutdown-symbolic");
    }

    #[test]
    fn confirmation_policy_matches_session_action_config() {
        let config = Config::default();

        assert!(dialogs::confirmation_spec(SessionAction::Lock, &config).is_none());
        assert!(dialogs::confirmation_spec(SessionAction::Logout, &config).is_some());
        assert!(dialogs::confirmation_spec(SessionAction::Suspend, &config).is_some());
        assert!(dialogs::confirmation_spec(SessionAction::Hibernate, &config).is_some());
        assert!(dialogs::confirmation_spec(SessionAction::Reboot, &config).is_some());
        assert!(dialogs::confirmation_spec(SessionAction::PowerOff, &config).is_some());
    }

    #[test]
    fn confirmation_policy_respects_disabled_confirmation_flags() {
        let config = Config {
            confirm_logout: false,
            confirm_suspend: false,
            confirm_hibernate: false,
            confirm_reboot: false,
            confirm_shutdown: false,
            ..Config::default()
        };

        assert!(dialogs::confirmation_spec(SessionAction::Logout, &config).is_none());
        assert!(dialogs::confirmation_spec(SessionAction::Suspend, &config).is_none());
        assert!(dialogs::confirmation_spec(SessionAction::Hibernate, &config).is_none());
        assert!(dialogs::confirmation_spec(SessionAction::Reboot, &config).is_none());
        assert!(dialogs::confirmation_spec(SessionAction::PowerOff, &config).is_none());
    }
}
