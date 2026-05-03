use std::collections::HashMap;

use gtk4::gdk;
use gtk4::prelude::{GtkWindowExt, OrientableExt, WidgetExt};
use gtk4_layer_shell::LayerShell;
use relm4::{Component, ComponentParts, ComponentSender, gtk};

pub mod applets;

use crate::panels::applets::{AppletController, AppletKey, build_applets, reconcile_applets};
use crate::services::framework::Services;
use glimpse_config::{AppletConfig, PanelConfig, Position, ThemeMode};

#[derive(PartialEq, Clone, Eq, Hash)]
pub struct PanelKey {
    pub index: usize,
    pub monitor: String,
    pub position: Position,
}

pub struct Init {
    pub config: PanelConfig,
    pub services: Services,
    pub monitor: Option<gdk::Monitor>,
    pub applet_configs: HashMap<String, AppletConfig>,
}

#[derive(Debug)]
pub enum Input {
    Reconfigure(PanelRuntimeConfig),
}

#[derive(Debug)]
pub struct PanelRuntimeConfig {
    pub config: PanelConfig,
    pub applet_configs: HashMap<String, AppletConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PanelSection {
    Left,
    Center,
    Right,
}

pub struct Panel {
    services: Services,
    applet_configs: HashMap<String, AppletConfig>,
    left: SectionState,
    center: SectionState,
    right: SectionState,
}

struct SectionState {
    container: gtk::Box,
    applets: HashMap<AppletKey, AppletController>,
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
            &left_box,
            &init.applet_configs,
            init.services.clone(),
        );
        let center_applets = build_applets(
            PanelSection::Center,
            &init.config.center,
            &center_box,
            &init.applet_configs,
            init.services.clone(),
        );
        let right_applets = build_applets(
            PanelSection::Right,
            &init.config.right,
            &right_box,
            &init.applet_configs,
            init.services.clone(),
        );
        let widgets = view_output!();
        let model = Panel {
            services: init.services,
            applet_configs: init.applet_configs,
            left: SectionState {
                container: left_box,
                applets: left_applets,
            },
            center: SectionState {
                container: center_box,
                applets: center_applets,
            },
            right: SectionState {
                container: right_box,
                applets: right_applets,
            },
        };

        root.present();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            Input::Reconfigure(runtime) => {
                tracing::debug!("panel config change, updating");
                apply_panel_config(root, &runtime.config);
                apply_theme_mode(root, &runtime.config.theme_mode);

                reconcile_applets(
                    PanelSection::Left,
                    &runtime.config.left,
                    &self.left.container,
                    &mut self.left.applets,
                    &self.applet_configs,
                    &runtime.applet_configs,
                    self.services.clone(),
                );
                reconcile_applets(
                    PanelSection::Center,
                    &runtime.config.center,
                    &self.center.container,
                    &mut self.center.applets,
                    &self.applet_configs,
                    &runtime.applet_configs,
                    self.services.clone(),
                );
                reconcile_applets(
                    PanelSection::Right,
                    &runtime.config.right,
                    &self.right.container,
                    &mut self.right.applets,
                    &self.applet_configs,
                    &runtime.applet_configs,
                    self.services.clone(),
                );

                self.applet_configs = runtime.applet_configs;
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

fn apply_panel_config(window: &gtk::Window, config: &PanelConfig) {
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
