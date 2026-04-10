use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct NotificationsPersistedState {
    #[serde(default)]
    dnd: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct GlimpsePersistedState {
    #[serde(default)]
    notifications: NotificationsPersistedState,
}

pub fn notifications_state_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("glimpse")
        .join("state.json")
}

pub fn load_notifications_dnd() -> bool {
    load_notifications_dnd_from(notifications_state_path())
}

pub fn load_notifications_dnd_from(path: impl AsRef<Path>) -> bool {
    fs::read_to_string(path)
        .ok()
        .and_then(|contents| serde_json::from_str::<GlimpsePersistedState>(&contents).ok())
        .map(|state| state.notifications.dnd)
        .unwrap_or(false)
}

pub fn save_notifications_dnd(dnd: bool) -> io::Result<()> {
    save_notifications_dnd_to(notifications_state_path(), dnd)
}

pub fn save_notifications_dnd_to(path: impl AsRef<Path>, dnd: bool) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let state = GlimpsePersistedState {
        notifications: NotificationsPersistedState { dnd },
    };
    let contents = serde_json::to_string(&state)
        .expect("serializing the notifications persistence state cannot fail");

    fs::write(path, contents)
}
