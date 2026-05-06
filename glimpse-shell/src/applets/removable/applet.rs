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
        storage::{Command, State, StorageHandle},
    },
};

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    show_when_empty: bool,
    label_format: String,
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
                tracing::warn!(?error, "invalid removable applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_when_empty: false,
            label_format: String::new(),
            tooltip_format: "{count} removable device(s), {mounted} mounted".into(),
        }
    }
}

pub struct Applet {
    visible: bool,
    config: Config,
    tooltip: String,
    label: String,
    icon_name: String,
    state: State,
    service: StorageHandle,
    popover: Controller<Popover>,
    popover_open: bool,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: StorageHandle,
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
            add_css_class: "removable",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            #[watch]
            set_visible: model.visible,
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
            visible: applet_visible(&config, &state),
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

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::ServiceStateChanged(state) => {
                self.apply_state(state);
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.apply_state(self.service.snapshot());
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverOutput(PopoverOutput::Opened) => {
                self.popover_open = true;
                self.sync_popover();
                self.send_command(Command::Refresh);
            }
            Input::PopoverOutput(PopoverOutput::Closed) => {
                self.popover_open = false;
            }
            Input::PopoverOutput(PopoverOutput::Command(command)) => {
                self.send_command(command);
            }
        }
    }
}

impl Applet {
    fn apply_state(&mut self, state: State) {
        self.visible = applet_visible(&self.config, &state);
        self.icon_name = icon_name_for_state(&state);
        self.label = format::label(&self.config.label_format, &state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
        self.state = state.clone();
        if self.popover_open {
            self.popover.emit(PopoverInput::UpdateState(state));
        }
    }

    fn sync_popover(&self) {
        self.popover
            .emit(PopoverInput::UpdateState(self.state.clone()));
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send storage command");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn applet_visible(config: &Config, state: &State) -> bool {
    config.show_when_empty || !state.devices.is_empty()
}

fn icon_name_for_state(state: &State) -> String {
    state
        .devices
        .iter()
        .find_map(|device| (!device.icon.is_empty()).then(|| device.icon.clone()))
        .unwrap_or_else(|| "drive-removable-media-symbolic".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::storage::StorageDevice;

    #[test]
    fn default_config_hides_empty_applet() {
        assert!(!applet_visible(&Config::default(), &State::default()));
    }

    #[test]
    fn applet_is_visible_when_devices_exist() {
        let state = State {
            devices: vec![StorageDevice {
                id: "device".into(),
                name: "USB Drive".into(),
                ..StorageDevice::default()
            }],
            ..State::default()
        };

        assert!(applet_visible(&Config::default(), &state));
    }

    #[test]
    fn applet_icon_uses_device_icon_or_generic_fallback() {
        assert_eq!(
            icon_name_for_state(&State::default()),
            "drive-removable-media-symbolic"
        );

        let state = State {
            devices: vec![StorageDevice {
                icon: "media-flash-sd-mmc-symbolic".into(),
                ..StorageDevice::default()
            }],
            ..State::default()
        };

        assert_eq!(icon_name_for_state(&state), "media-flash-sd-mmc-symbolic");
    }
}
