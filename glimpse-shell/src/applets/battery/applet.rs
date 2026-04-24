use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;

use crate::panels::applets::AppletConfig;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    show_icon: bool,
    label_format: String,
    label_format_on_ac: String,
    tooltip_format: String,
    tooltip_format_on_ac: String,
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
            label_format: "".into(),
            label_format_on_ac: "".into(),
            tooltip_format: "{percentage}% {state}, {time_left}".into(),
            tooltip_format_on_ac: "{percentage}% {state}".into(),
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
}

#[derive(Debug)]
pub struct Init {
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
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
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Reconfigure(config) => {
                self.config = config;
            }
            Input::TogglePopover => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_raw_uses_defaults_when_missing() {
        assert_eq!(Config::from_raw(&None), Config::default());
    }

    #[test]
    fn config_from_raw_parses_valid_settings() {
        let raw = Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(toml::map::Map::from_iter([
                ("show_icon".into(), toml::Value::Boolean(false)),
                (
                    "settings_command".into(),
                    toml::Value::String("power-settings".into()),
                ),
            ])),
        });

        let config = Config::from_raw(&raw);

        assert!(!config.show_icon);
        assert_eq!(config.settings_command, "power-settings");
        assert_eq!(config.tooltip_format, Config::default().tooltip_format);
    }

    #[test]
    fn config_from_raw_falls_back_to_defaults_on_invalid_settings() {
        let raw = Some(AppletConfig {
            extends: None,
            settings: toml::Value::Table(toml::map::Map::from_iter([(
                "show_icon".into(),
                toml::Value::String("nope".into()),
            )])),
        });

        assert_eq!(Config::from_raw(&raw), Config::default());
    }
}
