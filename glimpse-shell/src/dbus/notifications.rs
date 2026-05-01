use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use zbus::{
    fdo::{RequestNameFlags, RequestNameReply},
    object_server::SignalEmitter,
    zvariant::OwnedValue,
};

use crate::services::notifications::{
    NotificationServerDispatcher,
    model::{NotificationAction, NotificationEntry, Signal},
};

pub const NOTIFICATIONS_BUS_NAME: &str = "org.freedesktop.Notifications";
pub const NOTIFICATIONS_OBJECT_PATH: &str = "/org/freedesktop/Notifications";

#[derive(Clone)]
pub struct NotificationServer {
    dispatcher: NotificationServerDispatcher,
    next_id: Arc<AtomicU32>,
}

impl NotificationServer {
    pub(crate) fn new(dispatcher: NotificationServerDispatcher) -> Self {
        Self {
            dispatcher,
            next_id: Arc::new(AtomicU32::new(1)),
        }
    }

    fn next_notification_id(&self, replaces_id: u32) -> u32 {
        allocate_notification_id(&self.next_id, replaces_id)
    }
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotificationServer {
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        let id = self.next_notification_id(replaces_id);
        let entry =
            notification_entry_from_dbus(id, app_name, app_icon, summary, body, actions, hints);

        self.dispatcher
            .inject(entry)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;

        Ok(id)
    }

    async fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        self.dispatcher
            .close(id)
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }

    fn get_capabilities(&self) -> Vec<String> {
        [
            "actions",
            "body",
            "body-markup",
            "icon-static",
            "persistence",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "Glimpse".into(),
            "Glimpse".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        signal_emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        signal_emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn activation_token(
        signal_emitter: &SignalEmitter<'_>,
        id: u32,
        token: &str,
    ) -> zbus::Result<()>;
}

pub(crate) async fn register_server(
    session: &zbus::Connection,
    dispatcher: NotificationServerDispatcher,
) -> zbus::Result<SignalEmitter<'static>> {
    let _ = session
        .object_server()
        .remove::<NotificationServer, _>(NOTIFICATIONS_OBJECT_PATH)
        .await;
    if let Err(error) = session
        .object_server()
        .at(
            NOTIFICATIONS_OBJECT_PATH,
            NotificationServer::new(dispatcher),
        )
        .await
    {
        let _ = session.release_name(NOTIFICATIONS_BUS_NAME).await;
        return Err(error);
    }

    let reply = session
        .request_name_with_flags(
            NOTIFICATIONS_BUS_NAME,
            RequestNameFlags::ReplaceExisting | RequestNameFlags::DoNotQueue,
        )
        .await?;
    if !matches!(
        reply,
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
    ) {
        let _ = session
            .object_server()
            .remove::<NotificationServer, _>(NOTIFICATIONS_OBJECT_PATH)
            .await;
        let _ = session.release_name(NOTIFICATIONS_BUS_NAME).await;
        return Err(zbus::Error::NameTaken);
    }

    let iface_ref = session
        .object_server()
        .interface::<_, NotificationServer>(NOTIFICATIONS_OBJECT_PATH)
        .await?;
    Ok(iface_ref.signal_emitter().to_owned())
}

pub(crate) async fn unregister_server(session: &zbus::Connection) {
    let _ = session.release_name(NOTIFICATIONS_BUS_NAME).await;
    let _ = session
        .object_server()
        .remove::<NotificationServer, _>(NOTIFICATIONS_OBJECT_PATH)
        .await;
}

pub(crate) async fn emit_signal(
    signal_emitter: &SignalEmitter<'_>,
    signal: &Signal,
) -> zbus::Result<()> {
    match signal {
        Signal::NotificationClosed { id, reason } => {
            NotificationServer::notification_closed(signal_emitter, *id, *reason).await
        }
        Signal::ActionInvoked { id, action_key } => {
            NotificationServer::action_invoked(signal_emitter, *id, action_key).await
        }
        Signal::ActivationToken { id, token } => {
            NotificationServer::activation_token(signal_emitter, *id, token).await
        }
    }
}

fn notification_entry_from_dbus(
    id: u32,
    app_name: String,
    app_icon: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    hints: HashMap<String, OwnedValue>,
) -> NotificationEntry {
    NotificationEntry {
        id,
        app_name,
        app_icon,
        desktop_entry: hint_as::<String>(&hints, "desktop-entry"),
        summary,
        body,
        urgency: hint_as::<u8>(&hints, "urgency").unwrap_or(1),
        actions: parse_actions(&actions),
        image: hint_as::<String>(&hints, "image-path"),
        timestamp: now_ms(),
        resident: hint_as::<bool>(&hints, "resident").unwrap_or(false),
    }
}

fn hint_as<T>(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    hints
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| T::try_from(value).ok())
}

fn parse_actions(actions: &[String]) -> Vec<NotificationAction> {
    actions
        .chunks(2)
        .filter_map(|pair| match pair {
            [key, label] => Some(NotificationAction {
                key: key.clone(),
                label: label.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn allocate_notification_id(next_id: &AtomicU32, replaces_id: u32) -> u32 {
    if replaces_id != 0 {
        if let Some(target) = replaces_id.checked_add(1) {
            let mut current = next_id.load(Ordering::Relaxed);
            while current < target {
                match next_id.compare_exchange(
                    current,
                    target,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(observed) => current = observed,
                }
            }
        }

        return replaces_id;
    }

    loop {
        let current = next_id.load(Ordering::Relaxed);
        let allocated = if current == 0 { 1 } else { current };
        let next = allocated.checked_add(1).unwrap_or(1);

        match next_id.compare_exchange(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return allocated,
            Err(_) => continue,
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_ids_replace_existing_and_advance_allocator() {
        let next_id = AtomicU32::new(1);

        assert_eq!(allocate_notification_id(&next_id, 0), 1);
        assert_eq!(allocate_notification_id(&next_id, 7), 7);
        assert_eq!(allocate_notification_id(&next_id, 0), 8);
    }

    #[test]
    fn notification_ids_wrap_back_to_one_after_u32_max() {
        let next_id = AtomicU32::new(u32::MAX);

        assert_eq!(allocate_notification_id(&next_id, 0), u32::MAX);
        assert_eq!(allocate_notification_id(&next_id, 0), 1);
    }

    #[test]
    fn actions_parse_as_key_label_pairs_and_drop_incomplete_tail() {
        assert_eq!(
            parse_actions(&["default".into(), "Open".into(), "broken".into()]),
            vec![NotificationAction {
                key: "default".into(),
                label: "Open".into(),
            }]
        );
    }
}
