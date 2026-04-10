use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use glimpse::notifications::persistence::{
    load_notifications_dnd, load_notifications_dnd_from, notifications_state_path,
    save_notifications_dnd, save_notifications_dnd_to,
};
use serde_json::Value;

fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl AsRef<Path>) -> Self {
        let previous = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value.as_ref());
        }
        Self { key, previous }
    }

    fn remove(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        unsafe {
            std::env::remove_var(key);
        }
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

fn unique_dir(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!("glimpse-{label}-{stamp}"))
}

fn unique_state_file() -> PathBuf {
    unique_dir("notifications").join("state.json")
}

#[test]
fn missing_state_file_defaults_to_false() {
    let _lock = env_lock();
    let path = unique_state_file();

    assert!(!load_notifications_dnd_from(&path));
}

#[test]
fn malformed_state_file_defaults_to_false() {
    let _lock = env_lock();
    let path = unique_state_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, "{ not-json").unwrap();

    assert!(!load_notifications_dnd_from(&path));
}

#[test]
fn save_round_trips_nested_notifications_dnd_state_and_preserves_other_json() {
    let _lock = env_lock();
    let path = unique_state_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }

    fs::write(
        &path,
        r#"{"theme":"dark","notifications":{"dnd":false,"unrelated":123},"window":{"width":800}}"#,
    )
    .unwrap();

    save_notifications_dnd_to(&path, true).unwrap();

    let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["window"]["width"], 800);
    assert_eq!(value["notifications"]["unrelated"], 123);
    assert_eq!(value["notifications"]["dnd"], true);
    assert!(load_notifications_dnd_from(&path));

    save_notifications_dnd_to(&path, false).unwrap();

    let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["window"]["width"], 800);
    assert_eq!(value["notifications"]["unrelated"], 123);
    assert_eq!(value["notifications"]["dnd"], false);
    assert!(!load_notifications_dnd_from(&path));
}

#[test]
fn absolute_xdg_cache_home_takes_precedence() {
    let _lock = env_lock();
    let xdg_cache_home = unique_dir("xdg-cache");
    let home = unique_dir("home");
    let _xdg_guard = EnvGuard::set("XDG_CACHE_HOME", &xdg_cache_home);
    let _home_guard = EnvGuard::set("HOME", &home);

    assert_eq!(
        notifications_state_path(),
        Some(xdg_cache_home.join("glimpse").join("state.json"))
    );
}

#[test]
fn home_fallback_resolves_to_cache_path() {
    let _lock = env_lock();
    let home = unique_dir("home-fallback");
    let _xdg_guard = EnvGuard::remove("XDG_CACHE_HOME");
    let _home_guard = EnvGuard::set("HOME", &home);

    assert_eq!(
        notifications_state_path(),
        Some(home.join(".cache").join("glimpse").join("state.json"))
    );
}

#[test]
fn relative_xdg_cache_home_is_ignored() {
    let _lock = env_lock();
    let home = unique_dir("home-relative-xdg");
    let _xdg_guard = EnvGuard::set("XDG_CACHE_HOME", "relative-cache");
    let _home_guard = EnvGuard::set("HOME", &home);

    assert_eq!(
        notifications_state_path(),
        Some(home.join(".cache").join("glimpse").join("state.json"))
    );
}

#[test]
fn missing_envs_make_load_default_false_and_save_fail() {
    let _lock = env_lock();
    let _xdg_guard = EnvGuard::remove("XDG_CACHE_HOME");
    let _home_guard = EnvGuard::remove("HOME");

    assert_eq!(notifications_state_path(), None);
    assert!(!load_notifications_dnd());

    let error = save_notifications_dnd(true).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}
