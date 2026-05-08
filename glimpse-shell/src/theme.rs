use adw::gdk::{self};
use gio::prelude::SettingsExt;
use gtk4::{CssProvider, InterfaceColorScheme, glib::object::IsA, prelude::WidgetExt};
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
pub const THEME_DARK_CLASS: &str = "theme-dark";
pub const THEME_LIGHT_CLASS: &str = "theme-light";
pub const DIALOG_THEME_MODE: ThemeMode = ThemeMode::Dark;
const DIALOG_ADW_COLOR_SCHEME: adw::ColorScheme = adw::ColorScheme::ForceDark;
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
        if theme_file.exists() && theme_file.is_file() {
            tracing::info!(
                theme = %config.theme,
                mode = ?config.theme_mode,
                source = %theme_file.display(),
                "applied user theme"
            );
            self.theme.load_from_path(&theme_file);
        } else {
            tracing::debug!(
                theme = %config.theme,
                mode = ?config.theme_mode,
                source = %theme_file.display(),
                "user theme not found; using base theme only"
            );
            self.theme.load_from_string("");
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
    let gtk_scheme = match mode {
        ThemeMode::Auto => InterfaceColorScheme::Default,
        ThemeMode::Dark => InterfaceColorScheme::Dark,
        ThemeMode::Light => InterfaceColorScheme::Light,
    };

    (DIALOG_ADW_COLOR_SCHEME, gtk_scheme)
}

pub fn theme_mode_class(mode: &ThemeMode) -> Option<&'static str> {
    match mode {
        ThemeMode::Auto => None,
        ThemeMode::Dark => Some(THEME_DARK_CLASS),
        ThemeMode::Light => Some(THEME_LIGHT_CLASS),
    }
}

pub fn apply_theme_mode(widget: &impl IsA<gtk4::Widget>, mode: &ThemeMode) {
    widget.remove_css_class(THEME_DARK_CLASS);
    widget.remove_css_class(THEME_LIGHT_CLASS);
    if let Some(class) = theme_mode_class(mode) {
        widget.add_css_class(class);
    }
}

fn schemes_for_effective_mode(
    mode: EffectiveThemeMode,
) -> (adw::ColorScheme, InterfaceColorScheme) {
    let gtk_scheme = match mode {
        EffectiveThemeMode::Light => InterfaceColorScheme::Light,
        EffectiveThemeMode::Dark => InterfaceColorScheme::Dark,
    };

    (DIALOG_ADW_COLOR_SCHEME, gtk_scheme)
}

pub fn sync_system_color_scheme(mode: EffectiveThemeMode) -> Result<(), glib::BoolError> {
    let settings = gio::Settings::new(GNOME_INTERFACE_SCHEMA);
    settings.set_string(
        GNOME_COLOR_SCHEME_KEY,
        gsettings_color_scheme_for_effective_mode(mode),
    )
}

fn gsettings_color_scheme_for_effective_mode(mode: EffectiveThemeMode) -> &'static str {
    match mode {
        EffectiveThemeMode::Light => "prefer-light",
        EffectiveThemeMode::Dark => "prefer-dark",
    }
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
    fn configured_dark_forces_dialogs_and_css_provider_dark() {
        assert_eq!(
            schemes_for_configured_mode(&ThemeMode::Dark),
            (adw::ColorScheme::ForceDark, InterfaceColorScheme::Dark)
        );
    }

    #[test]
    fn configured_auto_forces_dialogs_and_leaves_css_provider_on_system_default() {
        assert_eq!(
            schemes_for_configured_mode(&ThemeMode::Auto),
            (adw::ColorScheme::ForceDark, InterfaceColorScheme::Default)
        );
    }

    #[test]
    fn configured_light_keeps_dialogs_dark_and_css_provider_light() {
        assert_eq!(
            schemes_for_configured_mode(&ThemeMode::Light),
            (adw::ColorScheme::ForceDark, InterfaceColorScheme::Light)
        );
    }

    #[test]
    fn theme_mode_class_maps_panel_override_classes() {
        assert_eq!(theme_mode_class(&ThemeMode::Dark), Some("theme-dark"));
        assert_eq!(theme_mode_class(&ThemeMode::Light), Some("theme-light"));
        assert_eq!(theme_mode_class(&ThemeMode::Auto), None);
    }

    #[test]
    fn effective_dark_forces_dialogs_and_css_provider_dark() {
        assert_eq!(
            schemes_for_effective_mode(EffectiveThemeMode::Dark),
            (adw::ColorScheme::ForceDark, InterfaceColorScheme::Dark)
        );
    }

    #[test]
    fn effective_light_keeps_dialogs_dark_and_css_provider_light() {
        assert_eq!(
            schemes_for_effective_mode(EffectiveThemeMode::Light),
            (adw::ColorScheme::ForceDark, InterfaceColorScheme::Light)
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
