use std::{
    path::Path,
    time::Duration,
};

use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use relm4::{ComponentSender, gtk::CssProvider};

use glimpse::config::Config;

use crate::app::{App, Input};

pub(super) fn load_css(provider: &CssProvider, path: Option<&Path>) {
    provider.load_from_data("");

    let Some(path) = path else {
        tracing::info!("no named theme configured; user theme css cleared");
        return;
    };

    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if resolved.exists() && resolved.is_file() {
        provider.load_from_path(&resolved);
        tracing::info!("loaded css from {}", resolved.display());
    } else {
        tracing::warn!("theme css file not found: {}", resolved.display());
    }
}

fn is_theme_css_change(config_dir: &Path, path: &Path) -> bool {
    if path.file_name().and_then(|name| name.to_str()) == Some("theme.css") {
        return true;
    }

    path.strip_prefix(config_dir).ok().is_some_and(|relative| {
        relative
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == "themes")
            && path.extension().and_then(|ext| ext.to_str()) == Some("css")
    })
}

pub(super) fn watch_for_config_changes(sender: ComponentSender<App>) {
    let config_dir = Config::config_directory();
    if !config_dir.exists() {
        tracing::error!("config directory {} does not exist", config_dir.display());
    }

    tracing::info!("watching config directory");

    relm4::spawn(async move {
        let watch_config_dir = config_dir.clone();
        let mut debouncer = match new_debouncer(
            Duration::from_millis(200),
            None,
            move |res: DebounceEventResult| {
                let events = match res {
                    Ok(events) => events,
                    Err(_) => return,
                };

                let mut config_changed = false;
                let mut css_changed = false;

                for event in events {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                            for path in &event.paths {
                                if let Some(filename) = path.file_name() {
                                    match filename.to_str() {
                                        Some("config.toml") => config_changed = true,
                                        Some("theme.css") => css_changed = true,
                                        _ => {}
                                    }
                                }
                                if is_theme_css_change(&watch_config_dir, path) {
                                    css_changed = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if config_changed {
                    tracing::debug!("config changed");
                    sender.input(Input::ConfigChanged(Config::load()));
                }
                if css_changed {
                    tracing::debug!("css changed");
                    sender.input(Input::CssChanged);
                }
            },
        ) {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("failed to create watcher: {}", e);
                return;
            }
        };

        if let Err(e) = debouncer.watch(&config_dir, notify::RecursiveMode::Recursive) {
            tracing::error!("failed to watch config directory: {}", e);
            return;
        }

        for (name, recursive_mode) in [
            ("theme.css", notify::RecursiveMode::NonRecursive),
            ("config.toml", notify::RecursiveMode::NonRecursive),
            ("themes", notify::RecursiveMode::Recursive),
        ] {
            let path = config_dir.join(name);
            if !path.is_symlink() {
                continue;
            }
            let Ok(resolved) = path.canonicalize() else {
                continue;
            };
            let Some(parent) = resolved.parent() else {
                continue;
            };
            if parent == config_dir {
                continue;
            }
            let watch_path = if name == "themes" {
                resolved
            } else {
                parent.to_path_buf()
            };
            if let Err(e) = debouncer.watch(&watch_path, recursive_mode) {
                tracing::warn!("failed to watch symlink target for {}: {}", name, e);
            } else {
                tracing::info!("watching symlink target: {}", watch_path.display());
            }
        }

        std::future::pending::<()>().await;
    });
}

#[cfg(test)]
mod tests {
    use super::is_theme_css_change;
    use std::path::Path;

    #[test]
    fn theme_change_detection_matches_named_theme_files_under_themes_directory() {
        let config_dir = Path::new("/tmp/glimpse");
        let theme_file = config_dir.join("themes").join("rose-pine.css");

        assert!(is_theme_css_change(config_dir, &theme_file));
    }

    #[test]
    fn theme_change_detection_ignores_non_css_files_under_themes_directory() {
        let config_dir = Path::new("/tmp/glimpse");
        let theme_file = config_dir.join("themes").join("README.md");

        assert!(!is_theme_css_change(config_dir, &theme_file));
    }

    #[test]
    fn theme_change_detection_keeps_legacy_theme_css_compatibility() {
        let config_dir = Path::new("/tmp/glimpse");
        let legacy_path = config_dir.join("theme.css");

        assert!(is_theme_css_change(config_dir, &legacy_path));
    }
}
