pub mod components;
pub mod heic;

use crate::compositor::detect;
use crate::config::BackdropConfig;
pub use crate::config::{ImageFit, WallpaperConfig, WallpaperMode};
use crate::display::connector_name;
pub use components::{BackdropWindow, BackdropWindowInit};
pub use components::{MonitorWindow, MonitorWindowInit};

use std::collections::HashMap;

use adw::prelude::*;
use relm4::gtk::gdk;
use relm4::{Component, Controller};

pub fn open_all_monitors(
    display: &gdk::Display,
    config: &WallpaperConfig,
) -> HashMap<String, Controller<MonitorWindow>> {
    let monitors = display.monitors();
    let mut windows = HashMap::new();

    for i in 0..monitors.n_items() {
        let Some(obj) = monitors.item(i) else {
            continue;
        };
        let Ok(monitor) = obj.downcast::<gdk::Monitor>() else {
            continue;
        };
        let name = connector_name(&monitor);

        let ctrl = MonitorWindow::builder()
            .launch(MonitorWindowInit {
                monitor,
                config: config.clone(),
            })
            .detach();

        windows.insert(name, ctrl);
    }

    windows
}

pub fn open_backdrop_all_monitors(
    display: &gdk::Display,
    config: &BackdropConfig,
) -> HashMap<String, Controller<BackdropWindow>> {
    let monitors = display.monitors();
    let mut windows = HashMap::new();

    if !config.enabled {
        return windows;
    }

    if !detect().capabilities().backdrop {
        return windows;
    }

    let Some(path) = config.path.clone() else {
        return windows;
    };

    if !path.exists() {
        tracing::warn!("backdrop configured but image file does not exist");
        return windows;
    }

    for i in 0..monitors.n_items() {
        let Some(obj) = monitors.item(i) else {
            continue;
        };
        let Ok(monitor) = obj.downcast::<gdk::Monitor>() else {
            continue;
        };
        let name = connector_name(&monitor);
        let ctrl = BackdropWindow::builder()
            .launch(BackdropWindowInit {
                path: path.clone(),
                monitor: monitor.clone(),
                blur_radius: config.blur_radius,
            })
            .detach();
        windows.insert(name.clone(), ctrl);
    }

    windows
}
