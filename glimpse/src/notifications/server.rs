use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;

use crate::notifications::protocol::NotificationEntry;
use crate::notifications::service::{NotificationsServiceHandle, NotificationsSignal};

pub(crate) const NOTIFICATIONS_BUS_NAME: &str = "org.freedesktop.Notifications";
pub(crate) const NOTIFICATIONS_OBJECT_PATH: &str = "/org/freedesktop/Notifications";

#[derive(Clone)]
pub struct NotificationServer {
    service: NotificationsServiceHandle,
    next_id: Arc<AtomicU32>,
}

impl NotificationServer {
    pub(crate) fn new(service: NotificationsServiceHandle) -> Self {
        Self {
            service,
            next_id: Arc::new(AtomicU32::new(1)),
        }
    }

    fn next_notification_id(&self, replaces_id: u32) -> u32 {
        if replaces_id == 0 {
            return self.next_id.fetch_add(1, Ordering::Relaxed);
        }

        let target = replaces_id.saturating_add(1);
        let mut current = self.next_id.load(Ordering::Relaxed);
        while current < target {
            match self.next_id.compare_exchange(
                current,
                target,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }

        replaces_id
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

        self.service
            .inject(entry)
            .await
            .map_err(map_service_error)?;

        Ok(id)
    }

    async fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        self.service
            .close_from_server(id)
            .await
            .map_err(map_service_error)
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
            "0.1.0".into(),
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
    service: NotificationsServiceHandle,
) -> zbus::Result<SignalEmitter<'static>> {
    let _ = session
        .object_server()
        .remove::<NotificationServer, _>(NOTIFICATIONS_OBJECT_PATH)
        .await;
    session
        .object_server()
        .at(NOTIFICATIONS_OBJECT_PATH, NotificationServer::new(service))
        .await?;
    session
        .request_name_with_flags(
            NOTIFICATIONS_BUS_NAME,
            zbus::fdo::RequestNameFlags::ReplaceExisting.into(),
        )
        .await?;

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
    signal: &NotificationsSignal,
) -> zbus::Result<()> {
    match signal {
        NotificationsSignal::NotificationClosed { id, reason } => {
            NotificationServer::notification_closed(signal_emitter, *id, *reason).await
        }
        NotificationsSignal::ActionInvoked { id, action_key } => {
            NotificationServer::action_invoked(signal_emitter, *id, action_key).await
        }
        NotificationsSignal::ActivationToken { id, token } => {
            NotificationServer::activation_token(signal_emitter, *id, token).await
        }
    }
}

fn map_service_error(error: anyhow::Error) -> zbus::fdo::Error {
    zbus::fdo::Error::Failed(error.to_string())
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
        .and_then(|value| T::try_from(value.clone()).ok())
}

fn parse_actions(actions: &[String]) -> Vec<(String, String)> {
    actions
        .chunks(2)
        .filter_map(|pair| match pair {
            [key, label] => Some((key.clone(), label.clone())),
            _ => None,
        })
        .collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
