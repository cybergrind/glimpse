pub mod components;

pub use components::{MonitorWindow, MonitorWindowInit};

use std::collections::HashMap;

use adw::prelude::*;
use glimpse::wallpaper::WallpaperConfig;
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

pub fn connector_name(monitor: &gdk::Monitor) -> String {
    monitor
        .connector()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("monitor-{}", monitor.geometry().x()))
}
