use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, glib, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        audio::{AudioHandle, Command, State, volume_icon},
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
    pub show_icon: bool,
    pub show_mic_indicator: bool,
    #[serde(alias = "label")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub scroll_step: u32,
    pub max_volume: u32,
    pub show_streams: bool,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid audio applet config, using defaults");
                Self::default()
            }
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_icon: true,
            show_mic_indicator: true,
            label_format: format::DEFAULT_LABEL_FORMAT.into(),
            tooltip_format: format::DEFAULT_TOOLTIP_FORMAT.into(),
            scroll_step: 10,
            max_volume: 100,
            show_streams: true,
        }
    }
}

pub struct Applet {
    config: Config,
    state: State,
    icon_name: String,
    label: String,
    tooltip: String,
    service: AudioHandle,
    popover: Controller<Popover>,
    popover_open: bool,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: AudioHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    Scroll(f64),
    ToggleMute,
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
            set_visible: model.state.available,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
                },
            },

            add_controller = gtk::GestureClick {
                set_button: 2,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::ToggleMute);
                },
            },

            add_controller = gtk::EventControllerScroll::new(
                gtk::EventControllerScrollFlags::VERTICAL
            ) {
                connect_scroll[sender] => move |_, _dx, dy| {
                    sender.input(Input::Scroll(dy));
                    glib::Propagation::Stop
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

            gtk::Image {
                set_icon_name: Some("microphone-sensitivity-muted-symbolic"),
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                add_css_class: "is-warning",
                #[watch]
                set_visible: model.config.show_mic_indicator && input_muted(&model.state),
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
                max_volume: init.config.max_volume,
                show_streams: init.config.show_streams,
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let state = init.service.snapshot();
        let model = Applet {
            icon_name: icon_name_for_state(&state).into(),
            label: format::label(&init.config.label_format, &state),
            tooltip: format::tooltip(&init.config.tooltip_format, &state),
            config: init.config,
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
                self.apply_state(state);
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.apply_state(self.service.snapshot());
                if self.popover_open {
                    self.sync_popover_config();
                }
            }
            Input::Scroll(dy) => {
                let Some(output) = self.state.default_output() else {
                    return;
                };

                let delta = if dy > 0.0 {
                    -(self.config.scroll_step as i64)
                } else {
                    self.config.scroll_step as i64
                };
                let volume =
                    (output.volume as i64 + delta).clamp(0, self.config.max_volume as i64) as u32;
                self.send_command(Command::SetOutputVolume(volume));
            }
            Input::ToggleMute => {
                self.send_command(Command::ToggleOutputMute);
            }
            Input::TogglePopover => {
                self.popover.emit(PopoverInput::Toggle);
            }
            Input::PopoverOutput(PopoverOutput::Opened) => {
                self.popover_open = true;
                self.sync_popover_config();
                self.sync_popover_state();
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
        self.icon_name = icon_name_for_state(&state).into();
        self.label = format::label(&self.config.label_format, &state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
        self.state = state.clone();
        if self.popover_open {
            self.popover.emit(PopoverInput::UpdateState(state));
        }
    }

    fn sync_popover_config(&self) {
        self.popover.emit(PopoverInput::Reconfigure {
            max_volume: self.config.max_volume,
            show_streams: self.config.show_streams,
        });
    }

    fn sync_popover_state(&self) {
        self.popover
            .emit(PopoverInput::UpdateState(self.state.clone()));
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send audio command");
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
    state
        .default_output()
        .map(|device| volume_icon(device.volume, device.muted))
        .unwrap_or("audio-volume-muted-symbolic")
}

fn input_muted(state: &State) -> bool {
    state
        .default_input()
        .map(|device| device.muted)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::audio::AudioDevice;

    #[test]
    fn config_accepts_absent_and_empty_settings() {
        assert_eq!(Config::from_raw(&None), Config::default());
        assert_eq!(
            Config::from_raw(&Some(AppletConfig::default())),
            Config::default()
        );
    }

    #[test]
    fn icon_follows_default_output() {
        let mut state = State {
            available: true,
            ..State::default()
        };
        state.outputs.push(AudioDevice {
            index: 1,
            name: "sink".into(),
            description: "Speakers".into(),
            volume: 80,
            muted: false,
            is_default: true,
            icon_name: "audio-speakers-symbolic".into(),
        });

        assert_eq!(icon_name_for_state(&state), "audio-volume-high-symbolic");
    }
}
