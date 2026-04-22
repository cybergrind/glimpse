use adw::gdk::{self, prelude::DisplayExt, prelude::MonitorExt};
use gio::prelude::ListModelExt;
use glib::object::CastNone;

#[derive(Clone, Debug)]
pub struct Monitor {
    pub connector: Option<String>,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale_factor: i32,
    pub refresh_rate_mhz: i32,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
}

impl Monitor {
    fn from_gdk(monitor: &gdk::Monitor) -> Self {
        let geom = monitor.geometry();
        Self {
            connector: monitor.connector().map(|s| s.to_string()),
            x: geom.x(),
            y: geom.y(),
            width: geom.width(),
            height: geom.height(),
            scale_factor: monitor.scale_factor(),
            refresh_rate_mhz: monitor.refresh_rate(),
            model: monitor.model().map(|s| s.to_string()),
            manufacturer: monitor.manufacturer().map(|s| s.to_string()),
        }
    }
}

/// Query the current monitor list from GDK. Must be called on the GTK main
/// thread.
pub fn list_monitors() -> Vec<Monitor> {
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let model = display.monitors();
    (0..model.n_items())
        .filter_map(|i| model.item(i).and_downcast::<gdk::Monitor>())
        .map(|m| Monitor::from_gdk(&m))
        .collect()
}

/// Look up a live `gdk::Monitor` by its connector name. Must be called on the
/// GTK main thread (GObjects are thread-affine). Use for wiring panels to a
/// specific output via `gtk4_layer_shell::set_monitor`.
pub fn gdk_monitor_by_connector(connector: &str) -> Option<gdk::Monitor> {
    let display = gdk::Display::default()?;
    let monitors = display.monitors();
    (0..monitors.n_items())
        .filter_map(|i| monitors.item(i).and_downcast::<gdk::Monitor>())
        .find(|m| m.connector().map(|c| c == connector).unwrap_or(false))
}
