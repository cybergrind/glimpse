use std::{fs, path::PathBuf, time::Duration};

use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use tokio::sync::mpsc;

pub async fn watch_config_file<T, F>(
    config_file: PathBuf,
    sender: mpsc::Sender<T>,
    label: &'static str,
    mut load_event: F,
) where
    T: Send + 'static,
    F: FnMut(&std::path::Path) -> T + Send + 'static,
{
    let watch_file = config_file
        .canonicalize()
        .unwrap_or_else(|_| config_file.clone());
    let Some(config_dir) = config_file.parent().map(PathBuf::from) else {
        tracing::error!("{label} config file has no parent directory");
        return;
    };

    if let Err(err) = fs::create_dir_all(&config_dir) {
        tracing::error!("failed to create {label} config directory: {err}");
        return;
    }

    tracing::info!(
        config_file = %watch_file.display(),
        "watching {label} config file for changes"
    );

    let handler_file = watch_file.clone();
    let handler_sender = sender.clone();
    let mut debouncer = match new_debouncer(
        Duration::from_millis(200),
        None,
        move |res: DebounceEventResult| {
            let events = match res {
                Ok(events) => events,
                Err(_) => return,
            };

            for event in events {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        if event
                            .paths
                            .iter()
                            .any(|path| path_matches(path, &handler_file))
                        {
                            let event = load_event(&handler_file);
                            if let Err(err) = handler_sender.try_send(event) {
                                tracing::error!(
                                    "failed to broadcast {label} config change to the app: {err}"
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        },
    ) {
        Ok(debouncer) => debouncer,
        Err(err) => {
            tracing::error!("failed to create {label} config watcher: {err}");
            return;
        }
    };

    if let Err(err) = debouncer.watch(&config_dir, notify::RecursiveMode::Recursive) {
        tracing::error!("failed to watch {label} config directory: {err}");
        return;
    }

    sender.closed().await;
}

fn path_matches(path: &std::path::Path, expected: &std::path::Path) -> bool {
    path == expected
        || path
            .canonicalize()
            .map(|path| path == expected)
            .unwrap_or(false)
}
