mod window;

use std::collections::HashMap;

use adw::prelude::*;
use relm4::gtk::gdk;
use tracing::warn;

pub mod widget;

pub use crate::config::{BackdropConfig, BackdropMode};
pub use widget::build_backdrop_widget;
pub use window::BackdropWindow;

use crate::compositor::detect as detect_compositor;

pub fn open_all_monitors(
    display: &gdk::Display,
    config: &BackdropConfig,
) -> HashMap<String, BackdropWindow> {
    if !config.enabled {
        return HashMap::new();
    }

    if !detect_compositor().capabilities().backdrop {
        warn!("backdrop: enabled in config but current compositor is not niri; disabling");
        return HashMap::new();
    }

    if let Err(error) = config.validate() {
        warn!(error = %error, "backdrop: invalid config; disabling");
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

        match BackdropWindow::open(monitor, config) {
            Ok(window) => {
                windows.insert(name, window);
            }
            Err(error) => {
                warn!(error = %error, "backdrop: failed to open backdrop windows; disabling");
                for (_, window) in windows.drain() {
                    window.close();
                }
                return HashMap::new();
            }
        }
    }

    windows
}

fn connector_name(monitor: &gdk::Monitor) -> String {
    monitor
        .connector()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("unknown-{}", monitor.model().unwrap_or_default()))
}
