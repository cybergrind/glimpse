pub mod components;

use std::collections::HashMap;

use adw::prelude::*;
use relm4::{Component, Controller};
use relm4::gtk::gdk;

use crate::compositor::detect as detect_compositor;
use crate::display::connector_name;
pub use crate::config::BackdropConfig;
pub use components::{BackdropWindow, BackdropWindowInit};

pub fn open_all_monitors(
    display: &gdk::Display,
    config: &BackdropConfig,
) -> HashMap<String, Controller<BackdropWindow>> {
    if !config.enabled {
        return HashMap::new();
    }

    if !detect_compositor().capabilities().backdrop {
        tracing::warn!("backdrop: enabled in config but current compositor does not support it");
        return HashMap::new();
    }

    let Some(path) = config.path.as_deref() else {
        tracing::warn!("backdrop: enabled but no image path configured");
        return HashMap::new();
    };

    if !path.is_file() {
        tracing::warn!(path = %path.display(), "backdrop: image file does not exist");
        return HashMap::new();
    }

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
        let ctrl = BackdropWindow::builder()
            .launch(BackdropWindowInit {
                monitor,
                path: path.to_path_buf(),
                blur_radius: config.blur_radius,
            })
            .detach();
        windows.insert(name, ctrl);
    }

    windows
}
