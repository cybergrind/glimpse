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
        mpris::{Command, MprisHandle, State},
    },
};

use super::{
    format,
    popover::{Popover, PopoverInit, PopoverInput, PopoverOutput},
};

const DEFAULT_MAX_ROWS: usize = 5;
const PANEL_LABEL_MAX_WIDTH_CHARS: i32 = 48;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(alias = "label")]
    pub label_format: String,
    #[serde(alias = "tooltip")]
    pub tooltip_format: String,
    pub hide_when_empty: bool,
    pub max_rows: usize,
    pub show_artwork: bool,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid mpris applet config, using defaults");
                Self::default()
            }
        }
    }

    fn normalized_max_rows(&self) -> usize {
        self.max_rows.clamp(1, 12)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            label_format: format::DEFAULT_LABEL_FORMAT.into(),
            tooltip_format: format::DEFAULT_TOOLTIP_FORMAT.into(),
            hide_when_empty: true,
            max_rows: DEFAULT_MAX_ROWS,
            show_artwork: true,
        }
    }
}

pub struct Applet {
    config: Config,
    label: String,
    tooltip: String,
    hidden: bool,
    state: State,
    service: MprisHandle,
    popover: Controller<Popover>,
    popover_open: bool,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: MprisHandle,
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
            #[watch]
            set_visible: !model.hidden,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::TogglePopover);
                },
            },

            gtk::Label {
                add_css_class: "mpris-label",
                set_max_width_chars: PANEL_LABEL_MAX_WIDTH_CHARS,
                set_ellipsize: gtk::pango::EllipsizeMode::End,
                set_single_line_mode: true,
                set_xalign: 0.0,
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
                max_rows: init.config.normalized_max_rows(),
                show_artwork: init.config.show_artwork,
            })
            .forward(sender.input_sender(), Input::PopoverOutput);

        let state = init.service.snapshot();
        let label = format::label(&init.config.label_format, &state);
        let tooltip = format::tooltip(&init.config.tooltip_format, &state);
        let hidden = init.config.hide_when_empty && label.is_empty();
        let model = Applet {
            config: init.config,
            label,
            tooltip,
            hidden,
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
            Input::ServiceStateChanged(state) => self.apply_state(state),
            Input::Reconfigure(config) => {
                self.config = config;
                if self.popover_open {
                    self.sync_popover_config();
                }
                self.apply_state(self.service.snapshot());
            }
            Input::TogglePopover => self.popover.emit(PopoverInput::Toggle),
            Input::PopoverOutput(PopoverOutput::Opened) => {
                self.popover_open = true;
                self.sync_popover_config();
                self.sync_popover_state();
            }
            Input::PopoverOutput(PopoverOutput::Closed) => {
                self.popover_open = false;
            }
            Input::PopoverOutput(output) => {
                if let Some(command) = command_for_popover_output(output) {
                    self.send_command(command);
                }
            }
        }
    }
}

impl Applet {
    fn apply_state(&mut self, state: State) {
        self.label = format::label(&self.config.label_format, &state);
        self.tooltip = format::tooltip(&self.config.tooltip_format, &state);
        self.hidden = self.config.hide_when_empty && self.label.is_empty();
        self.state = state.clone();
        if self.popover_open {
            self.popover.emit(PopoverInput::Update(state));
        }
    }

    fn sync_popover_config(&self) {
        self.popover.emit(PopoverInput::Reconfigure {
            max_rows: self.config.normalized_max_rows(),
            show_artwork: self.config.show_artwork,
        });
    }

    fn sync_popover_state(&self) {
        self.popover.emit(PopoverInput::Update(self.state.clone()));
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send mpris command");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

fn command_for_popover_output(output: PopoverOutput) -> Option<Command> {
    match output {
        PopoverOutput::Opened | PopoverOutput::Closed => None,
        PopoverOutput::Previous { player_id } => Some(Command::Previous { player_id }),
        PopoverOutput::PlayPause { player_id } => Some(Command::PlayPause { player_id }),
        PopoverOutput::Next { player_id } => Some(Command::Next { player_id }),
        PopoverOutput::Raise { player_id } => Some(Command::Raise { player_id }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_rows_is_clamped_to_a_small_popover_limit() {
        let mut config = Config {
            max_rows: 0,
            ..Default::default()
        };
        assert_eq!(config.normalized_max_rows(), 1);

        config.max_rows = 99;
        assert_eq!(config.normalized_max_rows(), 12);
    }

    #[test]
    fn popover_outputs_map_to_service_commands() {
        assert_eq!(
            command_for_popover_output(PopoverOutput::PlayPause {
                player_id: "spotify".into()
            }),
            Some(Command::PlayPause {
                player_id: "spotify".into()
            })
        );
    }
}
