use std::{
    path::Path,
    time::Duration,
};

use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use relm4::ComponentSender;

use glimpse::config::Config;

use crate::app::{App, Input};
#[cfg(feature = "dev")]
use crate::app::theme_runtime;

fn is_theme_css_change(config_dir: &Path, path: &Path) -> bool {
    path.strip_prefix(config_dir).ok().is_some_and(|relative| {
        relative
            .components()
            .next()
            .is_some_and(|component| component.as_os_str() == "themes")
            && path.extension().and_then(|ext| ext.to_str()) == Some("css")
    }) || is_repo_theme_css_change(path)
}

#[cfg(feature = "dev")]
fn is_repo_theme_css_change(path: &Path) -> bool {
    theme_runtime::is_repo_theme_css_change(path)
}

#[cfg(not(feature = "dev"))]
fn is_repo_theme_css_change(_path: &Path) -> bool {
    false
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

        #[cfg(feature = "dev")]
        {
            let repo_themes_dir = theme_runtime::repo_themes_directory();
            if repo_themes_dir.exists() {
                if let Err(error) =
                    debouncer.watch(&repo_themes_dir, notify::RecursiveMode::Recursive)
                {
                    tracing::warn!(
                        error = %error,
                        path = %repo_themes_dir.display(),
                        "failed to watch repo themes directory"
                    );
                } else {
                    tracing::info!("watching repo themes directory: {}", repo_themes_dir.display());
                }
            }
        }

        for (name, recursive_mode) in [
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
}
