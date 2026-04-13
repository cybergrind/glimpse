mod widget;
mod window;

use std::collections::HashMap;

use adw::prelude::*;
use relm4::gtk::gdk;

use crate::compositor::detect as detect_compositor;
pub use crate::config::BackdropConfig;
pub use widget::build_backdrop_widget;
pub use window::BackdropWindow;

pub fn open_all_monitors(
    display: &gdk::Display,
    config: &BackdropConfig,
) -> HashMap<String, BackdropWindow> {
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

        match BackdropWindow::open(monitor, config) {
            Ok(window) => {
                windows.insert(name, window);
            }
            Err(error) => {
                tracing::warn!(error = %error, "backdrop: failed to open backdrop windows; disabling");
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
