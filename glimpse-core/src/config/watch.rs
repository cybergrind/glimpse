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
    let target_dir = watch_file.parent().map(PathBuf::from);

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

    let mut watched_any = false;
    for dir in watch_dirs(config_dir, target_dir) {
        match debouncer.watch(&dir, notify::RecursiveMode::Recursive) {
            Ok(()) => watched_any = true,
            Err(err) => {
                tracing::error!(
                    config_dir = %dir.display(),
                    "failed to watch {label} config directory: {err}"
                );
            }
        }
    }
    if !watched_any {
        return;
    }

    sender.closed().await;
}

fn watch_dirs(config_dir: PathBuf, target_dir: Option<PathBuf>) -> Vec<PathBuf> {
    let mut dirs = vec![config_dir];
    if let Some(target_dir) = target_dir {
        if !dirs.iter().any(|dir| dir == &target_dir) {
            dirs.push(target_dir);
        }
    }
    dirs
}

fn path_matches(path: &std::path::Path, expected: &std::path::Path) -> bool {
    path == expected
        || path
            .canonicalize()
            .map(|path| path == expected)
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    use tokio::time::{Duration, timeout};

    #[test]
    fn watch_dirs_includes_config_and_symlink_target_dirs() {
        let config_dir = PathBuf::from("/config");
        let target_dir = PathBuf::from("/target");

        assert_eq!(
            watch_dirs(config_dir.clone(), Some(target_dir.clone())),
            vec![config_dir, target_dir]
        );
    }

    #[test]
    fn watch_dirs_deduplicates_matching_config_and_target_dirs() {
        let config_dir = PathBuf::from("/config");

        assert_eq!(
            watch_dirs(config_dir.clone(), Some(config_dir.clone())),
            vec![config_dir]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn watch_config_file_notices_changes_to_symlink_target() {
        let temp = TestDir::new("watch-symlink-target");
        let target = temp.file("target/config.toml");
        let link = temp.file("xdg/glimpse/config.toml");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::write(&target, "first").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let (tx, mut rx) = mpsc::channel(1);
        let task = tokio::spawn(watch_config_file(link, tx, "test", |path| {
            path.to_path_buf()
        }));

        tokio::time::sleep(Duration::from_millis(300)).await;
        fs::write(&target, "second").unwrap();

        let event = timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("watcher should emit config change")
            .expect("watcher channel should stay open");
        assert_eq!(event, target);

        drop(rx);
        task.await.unwrap();
    }

    struct TestDir {
        root: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!("glimpse-{name}-{suffix}"));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn file(&self, relative: &str) -> PathBuf {
            self.root.join(relative)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
