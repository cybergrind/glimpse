#![allow(unused_assignments)]

use std::collections::HashMap;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::panels::applets::AppletConfig;
use glimpse_core::services::{
    framework::ServiceCommand,
    keyboard::{Command, KeyboardHandle, KeyboardLayout, State},
};

use super::format;

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub labels: HashMap<String, String>,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid keyboard applet config, using defaults");
                Self::default()
            }
        }
    }
}

pub struct Applet {
    state: KeyboardState,
    view: View,
    service: KeyboardHandle,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: KeyboardHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    ActivateNext,
    Scroll { next: bool },
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
            set_visible: model.view.visible,
            #[watch]
            set_tooltip_text: if model.view.tooltip.is_empty() {
                None
            } else {
                Some(&model.view.tooltip)
            },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(Input::ActivateNext);
                },
            },

            gtk::Label {
                #[watch]
                set_label: &model.view.label,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        install_scroll_controller(&root, &sender);

        let Init {
            service,
            config: _config,
        } = init;
        let state = KeyboardState::from(&service.snapshot());
        let view = view_from_state(&state);
        let model = Applet {
            state,
            view,
            service,
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
                let state = KeyboardState::from(&state);
                if self.state != state {
                    self.state = state;
                    self.sync_view();
                }
            }
            Input::Reconfigure(_config) => {}
            Input::ActivateNext => {
                self.send_command(Command::NextLayout);
            }
            Input::Scroll { next } => {
                self.send_command(if next {
                    Command::NextLayout
                } else {
                    Command::PreviousLayout
                });
            }
        }
    }
}

impl Applet {
    fn sync_view(&mut self) {
        let view = view_from_state(&self.state);
        if self.view != view {
            self.view = view;
        }
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(%error, "failed to send keyboard command from applet");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyboardState {
    available: bool,
    current_layout: Option<KeyboardLayout>,
}

impl From<&State> for KeyboardState {
    fn from(state: &State) -> Self {
        Self {
            available: state.available,
            current_layout: state.current_layout.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct View {
    visible: bool,
    label: String,
    tooltip: String,
}

fn view_from_state(state: &KeyboardState) -> View {
    let Some(layout) = state.current_layout.as_ref().filter(|_| state.available) else {
        return View::default();
    };

    View {
        visible: true,
        label: format::layout_label(layout),
        tooltip: format::layout_tooltip(layout),
    }
}

fn install_scroll_controller(root: &gtk::Box, sender: &ComponentSender<Applet>) {
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL
            | gtk::EventControllerScrollFlags::HORIZONTAL
            | gtk::EventControllerScrollFlags::DISCRETE,
    );
    let sender = sender.clone();
    scroll.connect_scroll(move |_, dx, dy| {
        if let Some(next) = scroll_direction(dx, dy) {
            sender.input(Input::Scroll { next });
        }

        gtk::glib::Propagation::Stop
    });
    root.add_controller(scroll);
}

fn scroll_direction(dx: f64, dy: f64) -> Option<bool> {
    if dx == 0.0 && dy == 0.0 {
        return None;
    }

    if dx.abs() > dy.abs() {
        Some(dx > 0.0)
    } else {
        Some(dy > 0.0)
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
    fn config_parses_label_map() {
        let config = Config::from_raw(&Some(AppletConfig {
            settings: toml::toml! {
                [labels]
                us = "🇺🇸"
                "English (US)" = "EN"
            }
            .into(),
            ..AppletConfig::default()
        }));

        assert_eq!(config.labels.get("us"), Some(&"🇺🇸".into()));
        assert_eq!(config.labels.get("English (US)"), Some(&"EN".into()));
    }

    #[test]
    fn view_hides_when_keyboard_service_is_unavailable() {
        let state = State::default();

        assert!(!view_from_state(&KeyboardState::from(&state)).visible);
    }

    #[test]
    fn view_uses_normalized_service_layout() {
        let state = State {
            available: true,
            layouts: vec![layout(0, "US"), layout(1, "PL")],
            current_layout: Some(layout(1, "PL")),
            current_index: Some(1),
            ..State::default()
        };
        let view = view_from_state(&KeyboardState::from(&state));

        assert!(view.visible);
        assert_eq!(view.label, "PL");
        assert_eq!(view.tooltip, "pl");
    }

    #[test]
    fn view_ignores_applet_label_config_because_service_normalizes_labels() {
        let state = State {
            available: true,
            layouts: vec![layout(0, "US")],
            current_layout: Some(layout(0, "US")),
            current_index: Some(0),
            ..State::default()
        };
        let view = view_from_state(&KeyboardState::from(&state));

        assert_eq!(view.label, "US");
    }

    #[test]
    fn scroll_direction_uses_dominant_axis_and_ignores_zero_delta() {
        assert_eq!(scroll_direction(0.0, 0.0), None);
        assert_eq!(scroll_direction(0.1, 1.0), Some(true));
        assert_eq!(scroll_direction(0.1, -1.0), Some(false));
        assert_eq!(scroll_direction(1.0, 0.1), Some(true));
        assert_eq!(scroll_direction(-1.0, 0.1), Some(false));
    }

    fn layout(index: usize, label: &str) -> KeyboardLayout {
        KeyboardLayout {
            index,
            name: label.to_lowercase(),
            code: label.into(),
            label: label.into(),
        }
    }
}
