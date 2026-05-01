use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{Map, Value};

pub fn notifications_state_path() -> Option<PathBuf> {
    cache_dir().map(|dir| dir.join("glimpse").join("state.json"))
}

pub fn load_dnd() -> bool {
    notifications_state_path()
        .map(load_dnd_from)
        .unwrap_or(false)
}

pub fn load_dnd_from(path: impl AsRef<Path>) -> bool {
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

pub fn save_dnd(enabled: bool) -> io::Result<()> {
    let path = notifications_state_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "notifications state path requires an absolute XDG_CACHE_HOME or HOME",
        )
    })?;
    save_dnd_to(path, enabled)
}

pub fn save_dnd_to(path: impl AsRef<Path>, enabled: bool) -> io::Result<()> {
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
    ensure_object(notifications).insert("dnd".to_string(), Value::Bool(enabled));

    let contents = serde_json::to_string(&Value::Object(root))
        .expect("serializing notifications state cannot fail");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_state_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "glimpse-shell-notifications-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn dnd_state_round_trips_without_losing_other_json() {
        let path = unique_state_path();
        fs::write(
            &path,
            r#"{"theme":"dark","notifications":{"dnd":false,"other":12}}"#,
        )
        .unwrap();

        save_dnd_to(&path, true).unwrap();

        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["theme"], "dark");
        assert_eq!(value["notifications"]["other"], 12);
        assert!(load_dnd_from(&path));

        let _ = fs::remove_file(path);
    }
}
