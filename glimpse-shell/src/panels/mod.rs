use std::collections::HashMap;
use std::hash::Hash;

use gtk4::gdk;
use gtk4::prelude::{BoxExt, GtkWindowExt, OrientableExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::{Component, ComponentParts, ComponentSender, gtk};
use serde::Deserialize;

pub mod applets;

use crate::panels::applets::{
    AppletBlueprint, AppletConfig, AppletController, AppletKey, AppletType, create_applet,
};
use crate::{services::framework::Services, theme::ThemeMode};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Left,
    Top,
    Right,
    Bottom,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Margin {
    #[serde(default)]
    pub left: i32,
    #[serde(default)]
    pub right: i32,
    #[serde(default)]
    pub top: i32,
    #[serde(default)]
    pub bottom: i32,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default = "default_panel_size")]
    pub size: i32,
    pub monitor: Option<String>,
    pub position: Position,
    #[serde(default)]
    pub margin: Margin,
    #[serde(default = "default_panel_theme_mode")]
    pub theme_mode: ThemeMode,
    #[serde(default)]
    pub left: Vec<String>,
    #[serde(default)]
    pub center: Vec<String>,
    #[serde(default)]
    pub right: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            position: Position::Top,
            size: default_panel_size(),
            theme_mode: default_panel_theme_mode(),
            left: vec![],
            center: vec![],
            right: vec![],
            monitor: None,
            margin: Margin {
                left: 0,
                right: 0,
                top: 0,
                bottom: 0,
            },
        }
    }
}

pub fn default_panel_size() -> i32 {
    36
}

pub fn default_panel_theme_mode() -> ThemeMode {
    ThemeMode::Dark
}

#[derive(PartialEq, Clone, Eq, Hash)]
pub struct PanelKey {
    pub index: usize,
    pub monitor: String,
    pub position: Position,
}

pub struct Init {
    pub key: PanelKey,
    pub config: Config,
    pub services: Services,
    pub monitor: Option<gdk::Monitor>,
    pub applet_configs: HashMap<String, AppletConfig>,
}

#[derive(Debug)]
pub enum Input {
    Reconfigure(Config),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PanelSection {
    Left,
    Center,
    Right,
}

pub struct Panel {
    config: Config,
    left_applets: HashMap<AppletKey, AppletController>,
    center_applets: HashMap<AppletKey, AppletController>,
    right_applets: HashMap<AppletKey, AppletController>,
}

#[relm4::component(pub)]
impl Component for Panel {
    type Init = Init;
    type Input = Input;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Window {
            set_decorated: false,
            add_css_class: "panel",

            #[local_ref]
            layout -> gtk::CenterBox {
                set_hexpand: true,
                set_orientation: orientation_for_position(&init.config.position),
                set_start_widget: Some(&left_box),
                set_center_widget: Some(&center_box),
                set_end_widget: Some(&right_box),
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        tracing::info!(
            "configuring panel, position {:?}, {} applets",
            init.config.position,
            init.config.left.len() + init.config.center.len() + init.config.right.len()
        );
        init_layer_shell(&root);
        if let Some(monitor) = init.monitor.as_ref() {
            root.set_monitor(monitor);
        }
        apply_panel_config(&root, &init.config);
        apply_theme_mode(&root, &init.config.theme_mode);

        let layout_orientation = orientation_for_position(&init.config.position);
        let left_box = gtk::Box::builder()
            .orientation(layout_orientation)
            .spacing(4)
            .build();
        let center_box = gtk::Box::builder()
            .orientation(layout_orientation)
            .spacing(4)
            .build();
        let right_box = gtk::Box::builder()
            .orientation(layout_orientation)
            .spacing(4)
            .build();
        let layout = gtk::CenterBox::new();

        let left_applets = build_applets(
            PanelSection::Left,
            &init.config.left,
            &init.key,
            &left_box,
            &init.applet_configs,
            init.services.clone(),
        );
        let center_applets = build_applets(
            PanelSection::Center,
            &init.config.center,
            &init.key,
            &left_box,
            &init.applet_configs,
            init.services.clone(),
        );
        let right_applets = build_applets(
            PanelSection::Right,
            &init.config.right,
            &init.key,
            &left_box,
            &init.applet_configs,
            init.services.clone(),
        );
        let widgets = view_output!();
        let model = Panel {
            config: init.config,
            left_applets,
            center_applets,
            right_applets,
        };

        root.present();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            Input::Reconfigure(new_config) => {
                tracing::debug!("panel config change, updating");
                apply_panel_config(root, &new_config);
                apply_theme_mode(root, &new_config.theme_mode);
            }
        }
    }
}

fn init_layer_shell(window: &gtk::Window) {
    window.init_layer_shell();
    window.set_layer(gtk4_layer_shell::Layer::Top);
    window.set_namespace("glimpse-panel");
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    window.auto_exclusive_zone_enable();
}

fn apply_panel_config(window: &gtk::Window, config: &Config) {
    window.set_margin(gtk4_layer_shell::Edge::Top, config.margin.top);
    window.set_margin(gtk4_layer_shell::Edge::Right, config.margin.right);
    window.set_margin(gtk4_layer_shell::Edge::Bottom, config.margin.bottom);
    window.set_margin(gtk4_layer_shell::Edge::Left, config.margin.left);
    window.set_anchor(gtk4_layer_shell::Edge::Top, false);
    window.set_anchor(gtk4_layer_shell::Edge::Right, false);
    window.set_anchor(gtk4_layer_shell::Edge::Bottom, false);
    window.set_anchor(gtk4_layer_shell::Edge::Left, false);

    match config.position {
        Position::Top | Position::Bottom => {
            window.set_height_request(config.size);
            window.set_width_request(1);
        }
        Position::Left | Position::Right => {
            window.set_height_request(1);
            window.set_width_request(config.size);
        }
    }
    // if let Some(monitor) = config.monitor {
    //     window.set_mo
    // }

    match config.position {
        Position::Top => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
        Position::Right => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
        }
        Position::Bottom => {
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
        Position::Left => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
        }
    }
}

fn apply_theme_mode(window: &gtk::Window, mode: &ThemeMode) {
    window.remove_css_class("theme-dark");
    window.remove_css_class("theme-light");

    match mode {
        ThemeMode::Auto => {}
        ThemeMode::Dark => window.add_css_class("theme-dark"),
        ThemeMode::Light => window.add_css_class("theme-light"),
    }
}

fn orientation_for_position(position: &Position) -> gtk::Orientation {
    match position {
        Position::Top | Position::Bottom => gtk::Orientation::Horizontal,
        Position::Left | Position::Right => gtk::Orientation::Vertical,
    }
}

fn build_applets(
    section: PanelSection,
    configured_applets: &[String],
    _panel_key: &PanelKey,
    container: &gtk::Box,
    applet_configs: &HashMap<String, AppletConfig>,
    services: Services,
) -> HashMap<AppletKey, AppletController> {
    let mut applets = HashMap::new();
    let entries = collect_applets(section, configured_applets, applet_configs);
    for entry in entries {
        tracing::debug!(name = %entry.name, applet_type = ?entry.applet_type, "create applet");

        if let Some(applet) = create_applet(entry.clone(), services.clone()) {
            let widget = applet.widget();
            container.append(&widget);
            applets.insert(entry.key, applet);
        }
    }

    applets
}

fn collect_applets(
    section: PanelSection,
    configured: &[String],
    applet_configs: &HashMap<String, AppletConfig>,
) -> Vec<AppletBlueprint> {
    configured
        .iter()
        .enumerate()
        .filter_map(|(slot, name)| resolve_applet(section.clone(), slot, name, applet_configs))
        .collect()
}

fn resolve_applet(
    section: PanelSection,
    slot: usize,
    name: &str,
    applet_configs: &HashMap<String, AppletConfig>,
) -> Option<AppletBlueprint> {
    let applet_config = applet_configs.get(name).cloned();
    let applet_type = applet_config
        .as_ref()
        .and_then(|config| config.extends.clone())
        .or_else(|| AppletType::from_name(name));

    let Some(applet_type) = applet_type else {
        tracing::warn!(name, "unknown applet config, ignoring");
        return None;
    };

    let key = AppletKey {
        slot,
        section,
        name: name.to_string(),
        applet_type: applet_type.clone(),
    };

    Some(AppletBlueprint {
        slot,
        key,
        name: name.to_string(),
        applet_type,
        config: applet_config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_applets_uses_named_config_entry() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert(
            "laptop".to_string(),
            AppletConfig {
                extends: Some(AppletType::Battery),
                settings: toml::Value::Table(toml::map::Map::new()),
            },
        );

        let entries = collect_applets(PanelSection::Right, &["laptop".into()], &applet_configs);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "laptop");
        assert_eq!(entries[0].applet_type, AppletType::Battery);
        assert!(entries[0].config.is_some());
    }

    #[test]
    fn collect_applets_falls_back_to_builtin_name() {
        let entries = collect_applets(PanelSection::Left, &["battery".into()], &HashMap::new());

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "battery");
        assert_eq!(entries[0].applet_type, AppletType::Battery);
        assert!(entries[0].config.is_none());
    }

    #[test]
    fn collect_applets_uses_builtin_type_for_named_builtin_config_without_extends() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert("battery".to_string(), AppletConfig::default());

        let entries = collect_applets(PanelSection::Left, &["battery".into()], &applet_configs);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "battery");
        assert_eq!(entries[0].applet_type, AppletType::Battery);
        assert!(entries[0].config.is_some());
        assert_eq!(entries[0].config.as_ref().unwrap().extends, None);
    }

    #[test]
    fn collect_applets_ignores_unknown_named_config_without_extends() {
        let mut applet_configs = HashMap::new();
        applet_configs.insert("custom_battery".to_string(), AppletConfig::default());

        let entries = collect_applets(
            PanelSection::Right,
            &["custom_battery".into()],
            &applet_configs,
        );

        assert!(entries.is_empty());
    }
}
