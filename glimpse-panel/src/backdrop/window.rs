use anyhow::{Context, Result};
use adw::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::gtk::{self, gdk};

use glimpse::backdrop::{BackdropConfig, build_backdrop_widget};

pub struct BackdropWindow {
    window: gtk::Window,
}

impl BackdropWindow {
    pub fn open(monitor: gdk::Monitor, config: &BackdropConfig) -> Result<Self> {
        let window = gtk::Window::new();
        setup_layer_shell(&window, &monitor);

        let geometry = monitor.geometry();
        let child = build_backdrop_widget(config, geometry.width(), geometry.height())
            .with_context(|| format!("failed to build backdrop for {}", connector_name(&monitor)))?;
        window.set_child(Some(&child));
        window.present();

        Ok(Self { window })
    }

    pub fn close(self) {
        self.window.close();
    }
}

fn setup_layer_shell(window: &gtk::Window, monitor: &gdk::Monitor) {
    window.init_layer_shell();
    window.set_layer(Layer::Background);
    window.set_namespace("glimpse-backdrop");
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_exclusive_zone(-1);
    window.set_monitor(monitor);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_decorated(false);
    window.set_deletable(false);
}

fn connector_name(monitor: &gdk::Monitor) -> String {
    monitor
        .connector()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("unknown-{}", monitor.model().unwrap_or_default()))
}
