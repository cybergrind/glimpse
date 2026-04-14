use std::collections::HashMap;

use adw::prelude::*;
use relm4::{
    Component, ComponentController, Controller,
    gtk::{self, gdk::Display},
};

use glimpse::{backdrop, config::Config, display::connector_name, wallpaper};

pub(super) fn sync_background_windows(
    display: Option<Display>,
    config: &Config,
    wallpaper_windows: &mut HashMap<String, Controller<wallpaper::MonitorWindow>>,
    backdrop_windows: &mut HashMap<String, Controller<backdrop::BackdropWindow>>,
) {
    let Some(display) = display else {
        close_wallpaper_windows(wallpaper_windows);
        close_backdrop_windows(backdrop_windows);
        return;
    };

    sync_wallpaper_windows(&display, &config.wallpaper, wallpaper_windows);
    let resolved_backdrop = backdrop::resolved_config(&config.backdrop, &config.wallpaper);
    sync_backdrop_windows(&display, &resolved_backdrop, backdrop_windows);
}

fn close_wallpaper_windows(
    wallpaper_windows: &mut HashMap<String, Controller<wallpaper::MonitorWindow>>,
) {
    for (_, ctrl) in wallpaper_windows.drain() {
        ctrl.widget().close();
    }
}

fn close_backdrop_windows(
    backdrop_windows: &mut HashMap<String, Controller<backdrop::BackdropWindow>>,
) {
    for (_, window) in backdrop_windows.drain() {
        window.widget().close();
    }
}

fn sync_wallpaper_windows(
    display: &Display,
    config: &wallpaper::WallpaperConfig,
    wallpaper_windows: &mut HashMap<String, Controller<wallpaper::MonitorWindow>>,
) {
    let mut current = std::mem::take(wallpaper_windows);
    let mut next = HashMap::new();
    let monitors = display.monitors();

    for i in 0..monitors.n_items() {
        let Some(obj) = monitors.item(i) else {
            continue;
        };
        let Ok(monitor) = obj.downcast::<gtk::gdk::Monitor>() else {
            continue;
        };
        let name = connector_name(&monitor);
        if let Some(existing) = current.remove(&name) {
            existing.emit(wallpaper::MonitorWindowInput::Reconfigure(config.clone()));
            next.insert(name, existing);
            continue;
        }

        let controller = wallpaper::MonitorWindow::builder()
            .launch(wallpaper::MonitorWindowInit {
                monitor,
                config: config.clone(),
            })
            .detach();
        next.insert(name, controller);
    }

    for controller in current.into_values() {
        controller.widget().close();
    }

    *wallpaper_windows = next;
}

fn sync_backdrop_windows(
    display: &Display,
    config: &backdrop::BackdropConfig,
    backdrop_windows: &mut HashMap<String, Controller<backdrop::BackdropWindow>>,
) {
    if !backdrop::is_active_config(config) {
        close_backdrop_windows(backdrop_windows);
        return;
    }

    let mut current = std::mem::take(backdrop_windows);
    let mut next = HashMap::new();
    let monitors = display.monitors();

    for i in 0..monitors.n_items() {
        let Some(obj) = monitors.item(i) else {
            continue;
        };
        let Ok(monitor) = obj.downcast::<gtk::gdk::Monitor>() else {
            continue;
        };
        let name = connector_name(&monitor);
        if let Some(existing) = current.remove(&name) {
            existing.emit(backdrop::BackdropWindowInput::Reconfigure(config.clone()));
            next.insert(name, existing);
            continue;
        }

        let controller = backdrop::BackdropWindow::builder()
            .launch(backdrop::BackdropWindowInit {
                monitor,
                config: config.clone(),
            })
            .detach();
        next.insert(name, controller);
    }

    for controller in current.into_values() {
        controller.widget().close();
    }

    *backdrop_windows = next;
}
