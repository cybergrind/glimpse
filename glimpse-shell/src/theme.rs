use adw::gdk::{self};
use gio::prelude::SettingsExt;
use gtk4::{CssProvider, InterfaceColorScheme};
use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use std::cell::Cell;
#[cfg(feature = "dev")]
use std::path::Path;
use std::{path::PathBuf, time::Duration};
use tokio::sync::mpsc;

use glimpse_core::{Config, ThemeMode, services::theme::EffectiveThemeMode};

const RESOURCE_BASE: &str = "/me/aresa/GlimpseShell";
const GNOME_INTERFACE_SCHEMA: &str = "org.gnome.desktop.interface";
const GNOME_COLOR_SCHEME_KEY: &str = "color-scheme";
#[cfg(feature = "dev")]
const DEV_THEME_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../themes");

pub struct ThemeState {
    base: CssProvider,
    theme: CssProvider,
    provider_scheme: Cell<InterfaceColorScheme>,
}

impl ThemeState {
    /// Register both providers on the display once. Subsequent config changes
    /// should call `reload` — never call `install` twice or providers stack.
    pub fn install(config: &Config) -> Self {
        let display = gdk::Display::default().expect("no default display");

        let base = CssProvider::new();
        let theme = CssProvider::new();

        gtk4::style_context_add_provider_for_display(
            &display,
            &base,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        gtk4::style_context_add_provider_for_display(
            &display,
            &theme,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );

        let state = Self {
            base,
            theme,
            provider_scheme: Cell::new(InterfaceColorScheme::Default),
        };
        state.reload(config);
        state.apply_configured_mode(&config.theme_mode);
        state
    }

    /// Replace both providers' CSS content in place. Safe to call any time.
    pub fn reload(&self, config: &Config) {
        load_base_css(&self.base);

        let theme_file = config.theme_file();
        let shipped = format!("{RESOURCE_BASE}/themes/{}.css", config.theme);
        if theme_file.exists() && theme_file.is_file() {
            tracing::info!(
                theme = %config.theme,
                mode = ?config.theme_mode,
                source = %theme_file.display(),
                "applied user theme"
            );
            self.theme.load_from_path(&theme_file);
        } else if load_dev_theme_css(&self.theme, &config.theme, config) {
            return;
        } else if resource_exists(&shipped) {
            tracing::info!(
                theme = %config.theme,
                mode = ?config.theme_mode,
                source = %shipped,
                "applied shipped theme"
            );
            self.theme.load_from_resource(&shipped);
        } else {
            tracing::warn!(
                theme = %config.theme,
                mode = ?config.theme_mode,
                "theme not found, falling back to adwaita"
            );
            load_fallback_theme_css(&self.theme, config);
        }

        self.apply_provider_color_scheme(self.provider_scheme.get());
    }

    pub fn apply_effective_mode(&self, mode: EffectiveThemeMode) {
        let (adw_scheme, gtk_scheme) = schemes_for_effective_mode(mode);
        adw::StyleManager::default().set_color_scheme(adw_scheme);
        self.apply_provider_color_scheme(gtk_scheme);
    }

    pub fn apply_configured_mode(&self, mode: &ThemeMode) {
        let (adw_scheme, gtk_scheme) = schemes_for_configured_mode(mode);
        adw::StyleManager::default().set_color_scheme(adw_scheme);
        self.apply_provider_color_scheme(gtk_scheme);
    }

    fn apply_provider_color_scheme(&self, scheme: InterfaceColorScheme) {
        self.provider_scheme.set(scheme);
        self.base.set_prefers_color_scheme(scheme);
        self.theme.set_prefers_color_scheme(scheme);
    }
}

fn schemes_for_configured_mode(mode: &ThemeMode) -> (adw::ColorScheme, InterfaceColorScheme) {
    match mode {
        ThemeMode::Auto => (adw::ColorScheme::Default, InterfaceColorScheme::Default),
        ThemeMode::Dark => (adw::ColorScheme::ForceDark, InterfaceColorScheme::Dark),
        ThemeMode::Light => (adw::ColorScheme::ForceLight, InterfaceColorScheme::Light),
    }
}

fn schemes_for_effective_mode(
    mode: EffectiveThemeMode,
) -> (adw::ColorScheme, InterfaceColorScheme) {
    match mode {
        EffectiveThemeMode::Light => (adw::ColorScheme::ForceLight, InterfaceColorScheme::Light),
        EffectiveThemeMode::Dark => (adw::ColorScheme::ForceDark, InterfaceColorScheme::Dark),
    }
}

pub fn sync_system_color_scheme(mode: EffectiveThemeMode) -> Result<(), glib::BoolError> {
    let settings = gio::Settings::new(GNOME_INTERFACE_SCHEMA);
    settings.set_string(GNOME_COLOR_SCHEME_KEY, gsettings_color_scheme_for_effective_mode(mode))
}

fn gsettings_color_scheme_for_effective_mode(mode: EffectiveThemeMode) -> &'static str {
    match mode {
        EffectiveThemeMode::Light => "prefer-light",
        EffectiveThemeMode::Dark => "prefer-dark",
    }
}

fn resource_exists(path: &str) -> bool {
    gio::resources_get_info(path, gio::ResourceLookupFlags::NONE).is_ok()
}

fn load_base_css(provider: &CssProvider) {
    #[cfg(feature = "dev")]
    {
        let path = dev_theme_path("base.css");
        if path.is_file() {
            tracing::info!(source = %path.display(), "applied dev base theme");
            provider.load_from_path(path);
            return;
        }
    }

    provider.load_from_resource(&format!("{RESOURCE_BASE}/themes/base.css"));
}

#[cfg(feature = "dev")]
fn load_dev_theme_css(provider: &CssProvider, theme: &str, config: &Config) -> bool {
    let path = dev_theme_path(format!("{theme}.css"));
    if path.is_file() {
        tracing::info!(
            theme,
            mode = ?config.theme_mode,
            source = %path.display(),
            "applied dev shipped theme"
        );
        provider.load_from_path(path);
        return true;
    }

    false
}

#[cfg(not(feature = "dev"))]
fn load_dev_theme_css(_provider: &CssProvider, _theme: &str, _config: &Config) -> bool {
    false
}

#[cfg(feature = "dev")]
fn load_fallback_theme_css(provider: &CssProvider, config: &Config) {
    let path = dev_theme_path("adwaita.css");
    if path.is_file() {
        tracing::info!(
            theme = %config.theme,
            mode = ?config.theme_mode,
            source = %path.display(),
            "applied dev fallback theme"
        );
        provider.load_from_path(path);
        return;
    }

    provider.load_from_resource(&format!("{RESOURCE_BASE}/themes/adwaita.css"));
}

#[cfg(not(feature = "dev"))]
fn load_fallback_theme_css(provider: &CssProvider, _config: &Config) {
    provider.load_from_resource(&format!("{RESOURCE_BASE}/themes/adwaita.css"));
}

#[cfg(feature = "dev")]
fn dev_theme_path(path: impl AsRef<Path>) -> PathBuf {
    Path::new(DEV_THEME_DIR).join(path)
}

/// Watch theme directories and emit `()` on any `.css` change.
/// Returns when `sender` is dropped / its receiver is closed.
pub async fn watch_user_themes(sender: mpsc::Sender<()>) {
    let theme_dirs = theme_watch_dirs();
    if theme_dirs.is_empty() {
        tracing::debug!("no theme directories to watch");
        sender.closed().await;
        return;
    }

    let handler_sender = sender.clone();
    let mut debouncer = match new_debouncer(
        Duration::from_millis(200),
        None,
        move |res: DebounceEventResult| {
            theme_change_handler(res, handler_sender.clone());
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("failed to create theme watcher: {e}");
            return;
        }
    };

    let mut watched_any = false;
    for themes_dir in theme_dirs {
        match debouncer.watch(&themes_dir, notify::RecursiveMode::Recursive) {
            Ok(()) => {
                watched_any = true;
                tracing::info!("watching theme directory: {}", themes_dir.display());
            }
            Err(e) => {
                tracing::error!(
                    path = %themes_dir.display(),
                    "failed to watch theme directory: {e}"
                );
            }
        }
    }

    if !watched_any {
        return;
    }

    sender.closed().await;
}

fn theme_watch_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    push_theme_watch_dir(&mut dirs, Config::themes_dir());

    #[cfg(feature = "dev")]
    push_theme_watch_dir(&mut dirs, PathBuf::from(DEV_THEME_DIR));

    dirs
}

fn push_theme_watch_dir(dirs: &mut Vec<PathBuf>, dir: PathBuf) {
    if !dir.is_dir() {
        tracing::debug!("theme directory does not exist: {}", dir.display());
        return;
    }
    if !dirs.iter().any(|existing| existing == &dir) {
        dirs.push(dir);
    }
}

fn theme_change_handler(res: DebounceEventResult, sender: mpsc::Sender<()>) {
    let events = match res {
        Ok(events) => events,
        Err(_) => return,
    };

    for event in events {
        let is_relevant_kind = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        );
        let touches_css = event
            .paths
            .iter()
            .any(|p| p.extension().map(|e| e == "css").unwrap_or(false));

        if is_relevant_kind && touches_css {
            if let Err(e) = sender.try_send(()) {
                tracing::debug!("dropped theme-change event: {e}");
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_dark_forces_adwaita_and_css_provider_dark() {
        assert_eq!(
            schemes_for_configured_mode(&ThemeMode::Dark),
            (adw::ColorScheme::ForceDark, InterfaceColorScheme::Dark)
        );
    }

    #[test]
    fn configured_auto_leaves_css_provider_on_system_default() {
        assert_eq!(
            schemes_for_configured_mode(&ThemeMode::Auto),
            (adw::ColorScheme::Default, InterfaceColorScheme::Default)
        );
    }

    #[test]
    fn effective_dark_forces_css_provider_dark() {
        assert_eq!(
            schemes_for_effective_mode(EffectiveThemeMode::Dark),
            (adw::ColorScheme::ForceDark, InterfaceColorScheme::Dark)
        );
    }

    #[test]
    fn effective_mode_maps_to_gnome_color_scheme() {
        assert_eq!(
            gsettings_color_scheme_for_effective_mode(EffectiveThemeMode::Light),
            "prefer-light"
        );
        assert_eq!(
            gsettings_color_scheme_for_effective_mode(EffectiveThemeMode::Dark),
            "prefer-dark"
        );
    }
}
