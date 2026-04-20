use gtk4::prelude::{GtkWindowExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib},
};
use serde::Deserialize;

use crate::{services::framework::Services, theme::ThemeMode};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
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
    #[serde(default = "default_panel_height")]
    pub height: i32,
    pub monitor: Option<String>,
    pub position: Position,
    pub margin: Margin,
    #[serde(default = "default_panel_theme_mode")]
    pub theme_mode: ThemeMode,
    pub left: Vec<String>,
    pub center: Vec<String>,
    pub right: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            position: Position::Top,
            height: default_panel_height(),
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

pub fn default_panel_height() -> i32 {
    42
}

pub fn default_panel_theme_mode() -> ThemeMode {
    ThemeMode::Dark
}

pub struct Init {
    pub config: Config,
    pub services: Services,
}

#[derive(Debug)]
pub enum Input {}

pub struct Panel {
    config: Config,
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for Panel {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        gtk::Window {
            set_decorated: false,
            set_width_request: 1,

            #[local_ref]
            revealer -> gtk::Revealer {
                set_transition_duration: 180,
                set_reveal_child: false,
                set_transition_type: reveal_transition_for_position(&init.config.position),

                #[wrap(Some)]
                set_child = &gtk::CenterBox {
                    set_hexpand: true,
                    set_start_widget: Some(&left_box),
                    set_center_widget: Some(&center_box),
                    set_end_widget: Some(&right_box),
                }
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
        apply_panel_config(&root, &init.config);
        apply_theme_mode(&root, &init.config.theme_mode);

        println!("{:?}", stringify!(init.config));

        let left_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();

        let center_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();

        let right_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();

        let revealer = gtk::Revealer::new();

        let widgets = view_output!();
        let model = Panel {
            config: init.config,
        };

        root.present();
        let revealer_clone = revealer.clone();
        glib::idle_add_local_once(move || {
            revealer_clone.set_reveal_child(true);
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {}
}

fn init_layer_shell(window: &gtk::Window) {
    window.init_layer_shell();
    window.set_layer(gtk4_layer_shell::Layer::Top);
    window.set_namespace("glimpse-panel");
    window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
    window.auto_exclusive_zone_enable();
}

fn apply_panel_config(window: &gtk::Window, config: &Config) {
    window.set_height_request(config.height);
    window.set_margin(gtk4_layer_shell::Edge::Top, config.margin.top);
    window.set_margin(gtk4_layer_shell::Edge::Left, config.margin.left);
    window.set_margin(gtk4_layer_shell::Edge::Right, config.margin.right);
    window.set_margin(gtk4_layer_shell::Edge::Bottom, config.margin.bottom);
    window.set_anchor(gtk4_layer_shell::Edge::Top, false);
    window.set_anchor(gtk4_layer_shell::Edge::Bottom, false);
    window.set_anchor(gtk4_layer_shell::Edge::Left, false);
    window.set_anchor(gtk4_layer_shell::Edge::Right, false);

    // if let Some(monitor) = config.monitor {
    //     window.set_mo
    // }

    match config.position {
        Position::Top => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Right, true);
        }
        Position::Left => {
            window.set_anchor(gtk4_layer_shell::Edge::Top, true);
            window.set_anchor(gtk4_layer_shell::Edge::Left, true);
            window.set_anchor(gtk4_layer_shell::Edge::Bottom, true);
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

fn reveal_transition_for_position(position: &Position) -> gtk::RevealerTransitionType {
    match position {
        Position::Top => gtk::RevealerTransitionType::SlideDown,
        Position::Bottom => gtk::RevealerTransitionType::SlideUp,
        Position::Left => gtk::RevealerTransitionType::SlideRight,
        Position::Right => gtk::RevealerTransitionType::SlideLeft,
    }
}
