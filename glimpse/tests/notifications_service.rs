use std::path::PathBuf;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use glimpse::notifications::protocol::{NotificationEntry, NotificationsCommand};
use glimpse::notifications::service::{NotificationsServiceHandle, NotificationsSignal};

fn unique_state_file() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();

    std::env::temp_dir().join(format!("glimpse-notifications-service-{stamp}.json"))
}

fn notification_entry(id: u32, summary: &str, resident: bool) -> NotificationEntry {
    NotificationEntry {
        id,
        app_name: "Test App".into(),
        app_icon: "test-app".into(),
        desktop_entry: Some("test-app".into()),
        summary: summary.into(),
        body: format!("{summary} body"),
        urgency: 1,
        actions: vec![("default".into(), "Open".into())],
        image: None,
        timestamp: 1_700_000_000_000,
        resident,
    }
}

#[tokio::test]
async fn set_dnd_updates_published_state_and_persists() {
    let path = unique_state_file();
    let handle = NotificationsServiceHandle::new_for_tests_with_persistence_path(path.clone());

    handle
        .send(NotificationsCommand::SetDnd(true))
        .await
        .unwrap();

    let state = handle.subscribe().borrow().clone();
    assert!(state.dnd);

    let contents = std::fs::read_to_string(path).unwrap();
    let value: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(value["notifications"]["dnd"], true);
}

#[tokio::test]
async fn dismiss_removes_notification_from_current_session_state() {
    let handle = NotificationsServiceHandle::new_for_tests();

    handle
        .inject(notification_entry(7, "hello", false))
        .await
        .unwrap();
    handle
        .send(NotificationsCommand::Dismiss { id: 7 })
        .await
        .unwrap();

    assert!(handle.subscribe().borrow().notifications.is_empty());
}

#[tokio::test]
async fn injecting_same_id_replaces_existing_notification() {
    let handle = NotificationsServiceHandle::new_for_tests();

    handle
        .inject(notification_entry(7, "original", false))
        .await
        .unwrap();
    handle
        .inject(notification_entry(7, "updated", false))
        .await
        .unwrap();

    let state = handle.subscribe().borrow().clone();
    assert_eq!(state.notifications.len(), 1);
    assert_eq!(state.notifications[0].id, 7);
    assert_eq!(state.notifications[0].summary, "updated");
}

#[tokio::test]
async fn invoke_action_emits_signals_and_closes_non_resident_notification() {
    let handle = NotificationsServiceHandle::new_for_tests();
    let mut signals = handle.subscribe_test_signals();

    handle
        .inject(notification_entry(9, "actionable", false))
        .await
        .unwrap();
    handle
        .send(NotificationsCommand::InvokeAction {
            id: 9,
            action_key: "default".into(),
            activation_token: Some("token-123".into()),
        })
        .await
        .unwrap();

    assert_eq!(
        signals.recv().await.unwrap(),
        NotificationsSignal::ActivationToken {
            id: 9,
            token: "token-123".into(),
        }
    );
    assert_eq!(
        signals.recv().await.unwrap(),
        NotificationsSignal::ActionInvoked {
            id: 9,
            action_key: "default".into(),
        }
    );
    assert_eq!(
        signals.recv().await.unwrap(),
        NotificationsSignal::NotificationClosed { id: 9, reason: 2 }
    );
    assert!(handle.subscribe().borrow().notifications.is_empty());
}

#[tokio::test]
async fn service_stops_once_last_external_handle_is_dropped() {
    let (handle, shutdown) = NotificationsServiceHandle::new_for_tests_with_shutdown_probe();
    let clone = handle.clone();

    drop(handle);
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!*shutdown.borrow());

    drop(clone);
    let mut shutdown = shutdown;
    tokio::time::timeout(Duration::from_secs(1), shutdown.changed())
        .await
        .unwrap()
        .unwrap();
    assert!(*shutdown.borrow());
}

#[tokio::test]
async fn dismiss_all_keeps_local_mutation_when_signal_emission_fails() {
    let handle = NotificationsServiceHandle::new_for_tests_with_signal_failure();

    handle
        .inject(notification_entry(1, "first", false))
        .await
        .unwrap();
    handle
        .inject(notification_entry(2, "second", false))
        .await
        .unwrap();

    handle.send(NotificationsCommand::DismissAll).await.unwrap();

    let state = handle.subscribe().borrow().clone();
    assert!(state.notifications.is_empty());
    assert!(matches!(
        state.health,
        glimpse::notifications::protocol::NotificationsServiceHealth::Degraded { .. }
    ));
}

#[tokio::test]
async fn invoke_action_keeps_local_close_when_signal_emission_fails() {
    let handle = NotificationsServiceHandle::new_for_tests_with_signal_failure();

    handle
        .inject(notification_entry(11, "actionable", false))
        .await
        .unwrap();

    handle
        .send(NotificationsCommand::InvokeAction {
            id: 11,
            action_key: "default".into(),
            activation_token: Some("broken-signal".into()),
        })
        .await
        .unwrap();

    let state = handle.subscribe().borrow().clone();
    assert!(state.notifications.is_empty());
    assert!(matches!(
        state.health,
        glimpse::notifications::protocol::NotificationsServiceHealth::Degraded { .. }
    ));
}
