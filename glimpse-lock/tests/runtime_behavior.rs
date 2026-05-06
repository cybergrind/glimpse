use glimpse_lock::{
    auth::SecretString,
    runtime::{APP_ID, GTK_APPLICATION_ID, GTK_PREVIEW_APPLICATION_ID, LockRuntime},
};

#[test]
fn app_ids_are_stable_and_distinct() {
    assert_eq!(APP_ID, "me.aresa.GlimpseLock");
    assert_eq!(GTK_APPLICATION_ID, "me.aresa.GlimpseLock.App");
    assert_eq!(GTK_PREVIEW_APPLICATION_ID, "me.aresa.GlimpseLock.Preview");
    assert_ne!(APP_ID, GTK_APPLICATION_ID);
    assert_ne!(APP_ID, GTK_PREVIEW_APPLICATION_ID);
    assert_ne!(GTK_APPLICATION_ID, GTK_PREVIEW_APPLICATION_ID);
}

#[test]
fn secret_debug_does_not_expose_password() {
    let secret = SecretString::new("correct horse battery staple");

    let debug = format!("{secret:?}");

    assert!(debug.contains("SecretString"));
    assert!(!debug.contains("correct"));
    assert!(!debug.contains("horse"));
    assert!(!debug.contains("battery"));
    assert!(!debug.contains("staple"));
}

#[test]
fn runtime_reset_clears_locked_and_auth_state() {
    let mut runtime = LockRuntime::default();

    runtime.mark_locked();
    runtime.mark_auth_success();
    assert!(runtime.can_unlock());

    runtime.reset();

    assert!(!runtime.can_unlock());
}

#[test]
fn unlock_is_allowed_only_after_lock_and_auth_success() {
    let mut runtime = LockRuntime::default();

    assert!(!runtime.can_unlock());
    runtime.mark_auth_success();
    assert!(!runtime.can_unlock());
    runtime.mark_locked();
    assert!(runtime.can_unlock());
}

#[test]
fn failed_auth_clears_pending_success() {
    let mut runtime = LockRuntime::default();

    runtime.mark_locked();
    runtime.mark_auth_success();
    runtime.mark_auth_failure();

    assert!(!runtime.can_unlock());
}

#[tokio::test]
async fn test_single_instance_guard_rejects_second_owner() {
    let name = format!("me.aresa.GlimpseLock.Test{}", std::process::id());
    let _guard = LockRuntime::acquire_single_instance_for_testing(&name)
        .await
        .expect("first test guard should acquire name");

    let second = LockRuntime::acquire_single_instance_for_testing(&name).await;

    assert!(second.is_err());
}
