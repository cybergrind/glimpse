use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use glimpse::notifications::persistence::{load_notifications_dnd_from, save_notifications_dnd_to};

fn unique_state_file() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();

    std::env::temp_dir()
        .join(format!("glimpse-notifications-{stamp}"))
        .join("state.json")
}

#[test]
fn missing_state_file_defaults_to_false() {
    let path = unique_state_file();

    assert!(!load_notifications_dnd_from(&path));
}

#[test]
fn malformed_state_file_defaults_to_false() {
    let path = unique_state_file();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, "{ not-json").unwrap();

    assert!(!load_notifications_dnd_from(&path));
}

#[test]
fn save_round_trips_nested_notifications_dnd_state() {
    let path = unique_state_file();

    save_notifications_dnd_to(&path, true).unwrap();

    let data = fs::read_to_string(&path).unwrap();
    assert_eq!(data, r#"{"notifications":{"dnd":true}}"#);
    assert!(load_notifications_dnd_from(&path));

    save_notifications_dnd_to(&path, false).unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        r#"{"notifications":{"dnd":false}}"#
    );
    assert!(!load_notifications_dnd_from(&path));
}
