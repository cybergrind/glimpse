pub mod components;

use std::collections::HashMap;

use adw::prelude::*;
use relm4::{Component, Controller};
use relm4::gtk::gdk;

use crate::compositor::detect as detect_compositor;
use crate::display::connector_name;
pub use crate::config::BackdropConfig;
pub use components::{BackdropWindow, BackdropWindowInit, BackdropWindowInput};

pub fn is_active_config(config: &BackdropConfig) -> bool {
    config.enabled
        && detect_compositor().capabilities().backdrop
        && config.path.as_ref().is_some_and(|path| path.is_file())
}

pub fn open_all_monitors(
    display: &gdk::Display,
    config: &BackdropConfig,
) -> HashMap<String, Controller<BackdropWindow>> {
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
                config: config.clone(),
            })
            .detach();
        windows.insert(name, ctrl);
    }

    windows
}
