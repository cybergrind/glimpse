pub mod components;

use std::collections::HashMap;

use adw::prelude::*;
use relm4::{Component, Controller};
use relm4::gtk::gdk;

use crate::compositor::detect as detect_compositor;
use crate::display::connector_name;
pub use crate::config::BackdropConfig;
pub use crate::config::WallpaperConfig;
pub use components::{BackdropWindow, BackdropWindowInit, BackdropWindowInput};

pub fn resolved_config(config: &BackdropConfig, wallpaper: &WallpaperConfig) -> BackdropConfig {
    let mut resolved = config.clone();
    if resolved.path.is_none() && resolved.enabled && wallpaper.path.is_some() {
        resolved.path = wallpaper.path.clone();
    }
    resolved
}

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

#[cfg(test)]
mod tests {
    use super::{BackdropConfig, WallpaperConfig, resolved_config};
    use crate::config::{ImageFit, WallpaperMode};
    use std::path::PathBuf;

    #[test]
    fn resolved_config_falls_back_to_wallpaper_image() {
        let wallpaper = WallpaperConfig {
            color: "#000000".into(),
            transition_ms: 800,
            mode: WallpaperMode::Image,
            path: Some(PathBuf::from("/tmp/wallpaper.png")),
            fit: ImageFit::Cover,
        };
        let backdrop = BackdropConfig {
            enabled: true,
            path: None,
            blur_radius: 20,
        };

        let resolved = resolved_config(&backdrop, &wallpaper);
        assert_eq!(resolved.path, Some(PathBuf::from("/tmp/wallpaper.png")));
    }

    #[test]
    fn resolved_config_falls_back_to_wallpaper_path_even_in_color_mode() {
        let wallpaper = WallpaperConfig {
            color: "#000000".into(),
            transition_ms: 800,
            mode: WallpaperMode::Color,
            path: Some(PathBuf::from("/tmp/wallpaper.png")),
            fit: ImageFit::Cover,
        };
        let backdrop = BackdropConfig {
            enabled: true,
            path: None,
            blur_radius: 20,
        };

        let resolved = resolved_config(&backdrop, &wallpaper);
        assert_eq!(resolved.path, Some(PathBuf::from("/tmp/wallpaper.png")));
    }
}
