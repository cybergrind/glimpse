use adw::gdk::{self};
use gtk4::CssProvider;
use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use std::time::Duration;
use tokio::sync::mpsc;

use glimpse_config::{Config, ThemeMode};

const RESOURCE_BASE: &str = "/me/aresa/GlimpseShell";

pub struct ThemeState {
    base: CssProvider,
    theme: CssProvider,
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

        let state = Self { base, theme };
        state.reload(config);
        state
    }

    /// Replace both providers' CSS content in place. Safe to call any time.
    pub fn reload(&self, config: &Config) {
        self.base
            .load_from_resource(&format!("{RESOURCE_BASE}/themes/base.css"));

        let theme_file = config.theme_file();
        let shipped = format!("{RESOURCE_BASE}/themes/{}.css", config.theme.name);
        if theme_file.exists() && theme_file.is_file() {
            tracing::info!(
                theme = %config.theme.name,
                mode = ?config.theme.mode,
                source = %theme_file.display(),
                "applied user theme"
            );
            self.theme.load_from_path(&theme_file);
        } else if resource_exists(&shipped) {
            tracing::info!(
                theme = %config.theme.name,
                mode = ?config.theme.mode,
                source = %shipped,
                "applied shipped theme"
            );
            self.theme.load_from_resource(&shipped);
        } else {
            tracing::warn!(
                theme = %config.theme.name,
                mode = ?config.theme.mode,
                "theme not found, falling back to adwaita"
            );
            self.theme
                .load_from_resource(&format!("{RESOURCE_BASE}/themes/adwaita.css"));
        }

        apply_mode(&config.theme.mode);
    }
}

fn apply_mode(mode: &ThemeMode) {
    let scheme = match mode {
        ThemeMode::Auto => adw::ColorScheme::Default,
        ThemeMode::Dark => adw::ColorScheme::ForceDark,
        ThemeMode::Light => adw::ColorScheme::ForceLight,
    };
    adw::StyleManager::default().set_color_scheme(scheme);
}

fn resource_exists(path: &str) -> bool {
    gio::resources_get_info(path, gio::ResourceLookupFlags::NONE).is_ok()
}

/// Watch the user themes directory and emit `()` on any `.css` change.
/// Returns when `sender` is dropped / its receiver is closed.
pub async fn watch_user_themes(sender: mpsc::Sender<()>) {
    let themes_dir = Config::themes_dir();
    if !themes_dir.is_dir() {
        tracing::debug!(
            "user theme directory does not exist: {}",
            themes_dir.display()
        );
        sender.closed().await;
        return;
    }

    tracing::info!("watching user theme directory: {}", themes_dir.display());

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

    if let Err(e) = debouncer.watch(&themes_dir, notify::RecursiveMode::Recursive) {
        tracing::error!("failed to watch theme directory: {e}");
        return;
    }

    sender.closed().await;
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
