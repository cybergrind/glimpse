use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

pub fn notifications_state_path() -> PathBuf {
    cache_dir()
        .expect("notifications state path requires HOME or XDG_CACHE_HOME")
        .join("glimpse")
        .join("state.json")
}

pub fn load_notifications_dnd() -> bool {
    load_notifications_dnd_from(notifications_state_path())
}

pub fn load_notifications_dnd_from(path: impl AsRef<Path>) -> bool {
    read_state_value(path.as_ref())
        .and_then(|value| {
            value
                .as_object()
                .and_then(|root| root.get("notifications"))
                .and_then(Value::as_object)
                .and_then(|notifications| notifications.get("dnd"))
                .and_then(Value::as_bool)
        })
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

    let mut root = read_state_value(path)
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    let notifications = root
        .entry("notifications".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    ensure_object(notifications).insert("dnd".to_string(), Value::Bool(dnd));

    let contents = serde_json::to_string(&Value::Object(root))
        .expect("serializing the notifications persistence state cannot fail");
    fs::write(path, contents)
}

fn cache_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(path));
    }

    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache"))
}

fn read_state_value(path: &Path) -> Option<Value> {
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }

    value
        .as_object_mut()
        .expect("value was just converted to an object")
}
