use std::fs::{self, OpenOptions};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

pub fn notifications_state_path() -> Option<PathBuf> {
    cache_dir().map(|dir| dir.join("glimpse").join("state.json"))
}

pub fn load_notifications_dnd() -> bool {
    notifications_state_path()
        .map(|path| load_notifications_dnd_from(path))
        .unwrap_or(false)
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
    let path = notifications_state_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "notifications state path requires an absolute XDG_CACHE_HOME or HOME",
        )
    })?;
    save_notifications_dnd_to(path, dnd)
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
    write_atomic(path, contents.as_bytes())
}

fn cache_dir() -> Option<PathBuf> {
    if let Some(path) = usable_absolute_dir(std::env::var_os("XDG_CACHE_HOME")) {
        return Some(path);
    }

    usable_absolute_dir(std::env::var_os("HOME")).map(|home| home.join(".cache"))
}

fn usable_absolute_dir(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    let path = value?;
    if path.is_empty() {
        return None;
    }

    let path = PathBuf::from(path);
    path.is_absolute().then_some(path)
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

fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "state file path must have a parent directory",
        )
    })?;

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_path = parent.join(format!(
        ".{}.tmp-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("state"),
        std::process::id(),
        unique
    ));

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);

    fs::rename(&temp_path, path).or_else(|error| {
        let _ = fs::remove_file(&temp_path);
        Err(error)
    })
}
