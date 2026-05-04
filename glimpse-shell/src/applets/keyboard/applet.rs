#![allow(unused_assignments)]

use std::collections::HashMap;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    compositors::KeyboardLayout,
    panels::applets::AppletConfig,
    services::{
        compositor::{Command, CompositorHandle, State},
        framework::ServiceCommand,
    },
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
    config: Config,
    state: KeyboardState,
    view: View,
    service: CompositorHandle,
    subscription_cancel: CancellationToken,
}

#[derive(Debug)]
pub struct Init {
    pub service: CompositorHandle,
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

        let state = KeyboardState::from(&init.service.snapshot());
        let view = view_from_state(&init.config, &state);
        let model = Applet {
            config: init.config,
            state,
            view,
            service: init.service,
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
                let state = KeyboardState::from(&state);
                if self.state != state {
                    self.state = state;
                    self.sync_view();
                }
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.sync_view();
            }
            Input::ActivateNext => {
                self.switch_layout(true);
            }
            Input::Scroll { next } => {
                self.switch_layout(next);
            }
        }
    }
}

impl Applet {
    fn sync_view(&mut self) {
        let view = view_from_state(&self.config, &self.state);
        if self.view != view {
            self.view = view;
        }
    }

    fn switch_layout(&self, next: bool) {
        let Some(target) = next_layout_index(self.state.current_index, &self.state.layouts, next)
        else {
            return;
        };

        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service
                .send(ServiceCommand::Command(Command::SetKeyboardLayout(target)))
                .await
            {
                tracing::warn!(%error, "failed to send compositor command from keyboard applet");
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
    current_index: Option<usize>,
    layouts: Vec<KeyboardLayout>,
    current_layout: Option<KeyboardLayout>,
}

impl From<&State> for KeyboardState {
    fn from(state: &State) -> Self {
        let current_layout = state
            .current_keyboard_layout
            .and_then(|index| {
                state
                    .keyboard_layouts
                    .iter()
                    .find(|layout| layout.index == index)
            })
            .cloned();

        Self {
            available: state.capabilities.keyboard_layouts && !state.keyboard_layouts.is_empty(),
            current_index: current_layout.as_ref().map(|layout| layout.index),
            layouts: state.keyboard_layouts.clone(),
            current_layout,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct View {
    visible: bool,
    label: String,
    tooltip: String,
}

fn view_from_state(config: &Config, state: &KeyboardState) -> View {
    let Some(layout) = state.current_layout.as_ref().filter(|_| state.available) else {
        return View::default();
    };

    View {
        visible: true,
        label: format::layout_label(layout, &config.labels),
        tooltip: format::layout_tooltip(layout),
    }
}

fn next_layout_index(
    current: Option<usize>,
    layouts: &[KeyboardLayout],
    next: bool,
) -> Option<usize> {
    if layouts.len() < 2 {
        return None;
    }

    let current = current?;
    let position = layouts.iter().position(|layout| layout.index == current)?;
    let target = if next {
        (position + 1) % layouts.len()
    } else {
        (position + layouts.len() - 1) % layouts.len()
    };

    Some(layouts[target].index)
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
    use glimpse_core::compositors::CompositorCapabilities;

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
    fn view_hides_without_keyboard_layout_capability() {
        let state = State {
            keyboard_layouts: vec![layout(0, "us")],
            current_keyboard_layout: Some(0),
            ..State::default()
        };

        assert!(!view_from_state(&Config::default(), &KeyboardState::from(&state)).visible);
    }

    #[test]
    fn view_hides_when_current_layout_is_unknown() {
        let state = State {
            capabilities: CompositorCapabilities {
                keyboard_layouts: true,
                ..CompositorCapabilities::default()
            },
            keyboard_layouts: vec![layout(0, "us")],
            current_keyboard_layout: Some(7),
            ..State::default()
        };

        assert!(!view_from_state(&Config::default(), &KeyboardState::from(&state)).visible);
    }

    #[test]
    fn view_uses_current_layout_code_by_default() {
        let state = State {
            capabilities: CompositorCapabilities {
                keyboard_layouts: true,
                ..CompositorCapabilities::default()
            },
            keyboard_layouts: vec![layout(0, "us"), layout(1, "pl")],
            current_keyboard_layout: Some(1),
            ..State::default()
        };
        let view = view_from_state(&Config::default(), &KeyboardState::from(&state));

        assert!(view.visible);
        assert_eq!(view.label, "PL");
        assert_eq!(view.tooltip, "pl");
    }

    #[test]
    fn view_uses_configured_label_override() {
        let state = State {
            capabilities: CompositorCapabilities {
                keyboard_layouts: true,
                ..CompositorCapabilities::default()
            },
            keyboard_layouts: vec![layout(0, "us")],
            current_keyboard_layout: Some(0),
            ..State::default()
        };
        let config = Config {
            labels: HashMap::from([("us".into(), "🇺🇸".into())]),
        };
        let view = view_from_state(&config, &KeyboardState::from(&state));

        assert_eq!(view.label, "🇺🇸");
    }

    #[test]
    fn next_layout_index_wraps() {
        let layouts = vec![layout(0, "us"), layout(1, "pl"), layout(2, "de")];

        assert_eq!(next_layout_index(Some(0), &layouts, true), Some(1));
        assert_eq!(next_layout_index(Some(2), &layouts, true), Some(0));
        assert_eq!(next_layout_index(Some(0), &layouts, false), Some(2));
        assert_eq!(next_layout_index(Some(0), &layouts[..1], true), None);
        assert_eq!(next_layout_index(None, &layouts, true), None);
    }

    #[test]
    fn next_layout_index_uses_actual_layout_indices() {
        let layouts = vec![layout(2, "us"), layout(4, "pl"), layout(7, "de")];

        assert_eq!(next_layout_index(Some(2), &layouts, true), Some(4));
        assert_eq!(next_layout_index(Some(7), &layouts, true), Some(2));
        assert_eq!(next_layout_index(Some(2), &layouts, false), Some(7));
        assert_eq!(next_layout_index(Some(1), &layouts, true), None);
    }

    #[test]
    fn scroll_direction_uses_dominant_axis_and_ignores_zero_delta() {
        assert_eq!(scroll_direction(0.0, 0.0), None);
        assert_eq!(scroll_direction(0.1, 1.0), Some(true));
        assert_eq!(scroll_direction(0.1, -1.0), Some(false));
        assert_eq!(scroll_direction(1.0, 0.1), Some(true));
        assert_eq!(scroll_direction(-1.0, 0.1), Some(false));
    }

    fn layout(index: usize, name: &str) -> KeyboardLayout {
        KeyboardLayout {
            index,
            name: name.into(),
        }
    }
}
