use adw::gdk::{self};
use gtk4::CssProvider;
use serde::Deserialize;

use crate::config::Config;

const RESOURCE_BASE: &str = "/me/aresa/GlimpseShell";

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct ThemeConfig {
    pub name: String,
    pub mode: ThemeMode,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: String::from("adwaita"),
            mode: ThemeMode::Auto,
        }
    }
}

pub fn apply_theme(config: &Config) {
    let display = gdk::Display::default().expect("no default display");

    let base_css = CssProvider::new();
    base_css.load_from_resource(&format!("{RESOURCE_BASE}/themes/base.css"));

    let theme_css = CssProvider::new();
    let theme_file = config.theme_file();
    let shipped = format!("{RESOURCE_BASE}/themes/{}.css", config.theme.name);
    if theme_file.exists() && theme_file.is_file() {
        theme_css.load_from_path(theme_file);
    } else if resource_exists(&shipped) {
        theme_css.load_from_resource(&shipped);
    } else {
        tracing::warn!(
            theme = %config.theme.name,
            "theme not found, falling back to adwaita"
        );
        theme_css.load_from_resource(&format!("{RESOURCE_BASE}/themes/adwaita.css"));
    }

    gtk4::style_context_add_provider_for_display(
        &display,
        &base_css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    gtk4::style_context_add_provider_for_display(
        &display,
        &theme_css,
        gtk4::STYLE_PROVIDER_PRIORITY_USER,
    );
}

fn resource_exists(path: &str) -> bool {
    gio::resources_get_info(path, gio::ResourceLookupFlags::NONE).is_ok()
}
