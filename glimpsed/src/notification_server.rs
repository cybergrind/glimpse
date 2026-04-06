use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::broker::BrokerMsg;
use crate::provider::ProviderEvent;

/// Messages sent to the notification server loop
pub enum NotifyMessage {
    Notify {
        id: u32,
        app_name: String,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::OwnedValue>,
        expire_timeout: i32,
    },
    Close {
        id: u32,
    },
    // Methods forwarded from the provider
    Dismiss {
        id: u32,
        reply: tokio::sync::oneshot::Sender<anyhow::Result<serde_json::Value>>,
    },
    DismissAll {
        reply: tokio::sync::oneshot::Sender<anyhow::Result<serde_json::Value>>,
    },
    InvokeAction {
        id: u32,
        action_key: String,
        reply: tokio::sync::oneshot::Sender<anyhow::Result<serde_json::Value>>,
    },
    SetDnd {
        enabled: bool,
        reply: tokio::sync::oneshot::Sender<anyhow::Result<serde_json::Value>>,
    },
    ClearHistory {
        reply: tokio::sync::oneshot::Sender<anyhow::Result<serde_json::Value>>,
    },
}

struct NotificationServer {
    tx: mpsc::Sender<NotifyMessage>,
    next_id: Arc<AtomicU32>,
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
        hints: HashMap<String, zbus::zvariant::OwnedValue>,
        expire_timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        let id = if replaces_id > 0 {
            replaces_id
        } else {
            self.next_id.fetch_add(1, Ordering::Relaxed)
        };

        self.tx
            .send(NotifyMessage::Notify {
                id,
                app_name,
                app_icon,
                summary,
                body,
                actions,
                hints,
                expire_timeout,
            })
            .await
            .map_err(|_| zbus::fdo::Error::Failed("server stopped".into()))?;

        Ok(id)
    }

    fn close_notification(&self, id: u32) {
        let _ = self.tx.try_send(NotifyMessage::Close { id });
    }

    fn get_capabilities(&self) -> Vec<String> {
        ["actions", "body", "body-markup", "icon-static", "persistence"]
            .iter()
            .map(|s| s.to_string())
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
        signal_ctxt: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        signal_ctxt: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}

async fn close_notification(
    id: u32,
    reason: u32,
    notifications: &mut HashMap<u32, serde_json::Value>,
    history: &mut std::collections::VecDeque<serde_json::Value>,
    history_limit: usize,
    iface_ref: &zbus::object_server::InterfaceRef<NotificationServer>,
) {
    if let Some(notif) = notifications.remove(&id) {
        tracing::info!(id, reason, "notification closed");
        history.push_front(serde_json::json!({
            "id": id,
            "app_name": notif["app_name"],
            "app_icon": notif["app_icon"],
            "summary": notif["summary"],
            "body": notif["body"],
            "urgency": notif["urgency"],
            "timestamp": notif["timestamp"],
            "close_reason": reason,
        }));
        while history.len() > history_limit {
            history.pop_back();
        }
        let signal_emitter = iface_ref.signal_emitter();
        let _ = NotificationServer::notification_closed(signal_emitter, id, reason).await;
    }
}

/// Creates the notification server channel and returns the sender.
/// Call `run()` to start the server loop.
pub fn create_channel() -> (mpsc::Sender<NotifyMessage>, mpsc::Receiver<NotifyMessage>) {
    mpsc::channel(256)
}

/// The D-Bus server runs for the lifetime of the daemon.
pub async fn run(
    cancel: tokio_util::sync::CancellationToken,
    broker_tx: mpsc::Sender<BrokerMsg>,
    mut notify_rx: mpsc::Receiver<NotifyMessage>,
    notify_tx: mpsc::Sender<NotifyMessage>,
) -> anyhow::Result<()> {
    let conn = zbus::Connection::session().await?;

    let next_id = Arc::new(AtomicU32::new(1));

    let server = NotificationServer {
        tx: notify_tx,
        next_id,
    };

    conn.object_server()
        .at("/org/freedesktop/Notifications", server)
        .await?;

    conn.request_name_with_flags(
        "org.freedesktop.Notifications",
        zbus::fdo::RequestNameFlags::ReplaceExisting.into(),
    )
    .await?;

    tracing::info!("notification-server: claimed org.freedesktop.Notifications");

    let iface_ref = conn
        .object_server()
        .interface::<_, NotificationServer>("/org/freedesktop/Notifications")
        .await?;

    // State managed here (always alive)
    let mut notifications: HashMap<u32, serde_json::Value> = HashMap::new();
    let mut history: std::collections::VecDeque<serde_json::Value> = std::collections::VecDeque::new();
    let mut dnd = false;

    let default_timeout: i32 = 5000;
    let history_limit = 100;

    // Expiry timer
    let expiry = tokio::time::sleep(std::time::Duration::from_secs(86400));
    tokio::pin!(expiry);

    let emit = |notifications: &HashMap<u32, serde_json::Value>,
                history: &std::collections::VecDeque<serde_json::Value>,
                dnd: bool,
                broker_tx: &mpsc::Sender<BrokerMsg>| {
        let status = serde_json::json!({ "dnd": dnd, "count": notifications.len() });
        let list: Vec<&serde_json::Value> = notifications.values().collect();
        let hist: Vec<&serde_json::Value> = history.iter().collect();

        let tx = broker_tx.clone();
        let list_json = serde_json::to_value(&list).unwrap_or_default();
        let hist_json = serde_json::to_value(&hist).unwrap_or_default();

        tokio::spawn(async move {
            let _ = tx.send(BrokerMsg::ProviderEvent(ProviderEvent {
                topic: "notifications.status".into(),
                data: status,
            })).await;
            let _ = tx.send(BrokerMsg::ProviderEvent(ProviderEvent {
                topic: "notifications.list".into(),
                data: list_json,
            })).await;
            let _ = tx.send(BrokerMsg::ProviderEvent(ProviderEvent {
                topic: "notifications.history".into(),
                data: hist_json,
            })).await;
        });
    };

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            Some(msg) = notify_rx.recv() => {
                match msg {
                    NotifyMessage::Notify { id, app_name, app_icon, summary, body, actions, hints, expire_timeout } => {
                        let urgency = hints.get("urgency")
                            .and_then(|v| u8::try_from(v.clone()).ok())
                            .unwrap_or(1);
                        let desktop_entry = hints.get("desktop-entry")
                            .and_then(|v| String::try_from(v.clone()).ok());
                        let image = hints.get("image-path")
                            .and_then(|v| String::try_from(v.clone()).ok());
                        let resident = hints.get("resident")
                            .and_then(|v| bool::try_from(v.clone()).ok())
                            .unwrap_or(false);
                        let category = hints.get("category")
                            .and_then(|v| String::try_from(v.clone()).ok());

                        let action_pairs: Vec<(String, String)> = actions.chunks(2)
                            .filter_map(|c| if c.len() == 2 { Some((c[0].clone(), c[1].clone())) } else { None })
                            .collect();

                        let timeout = if expire_timeout < 0 { default_timeout } else { expire_timeout };

                        tracing::info!(id, app = %app_name, summary = %summary, urgency, "notification received");

                        let notif = serde_json::json!({
                            "id": id,
                            "app_name": app_name,
                            "app_icon": app_icon,
                            "desktop_entry": desktop_entry,
                            "summary": summary,
                            "body": body,
                            "urgency": urgency,
                            "category": category,
                            "actions": action_pairs,
                            "image": image,
                            "timestamp": glimpse_types::now_ms(),
                            "resident": resident,
                            "expire_timeout": timeout,
                        });
                        notifications.insert(id, notif);

                        // Reset expiry timer
                        let now = glimpse_types::now_ms();
                        let next_expiry = notifications.values()
                            .filter_map(|n| {
                                let t = n["expire_timeout"].as_i64().unwrap_or(0);
                                if t > 0 {
                                    let deadline = n["timestamp"].as_u64().unwrap_or(0) + t as u64;
                                    Some(deadline.saturating_sub(now))
                                } else { None }
                            })
                            .min()
                            .unwrap_or(86400_000);
                        expiry.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(next_expiry.max(10)));

                        emit(&notifications, &history, dnd, &broker_tx);
                    }
                    NotifyMessage::Close { id } => {
                        close_notification(id, 3, &mut notifications, &mut history, history_limit, &iface_ref).await;
                        emit(&notifications, &history, dnd, &broker_tx);
                    }
                    NotifyMessage::Dismiss { id, reply } => {
                        tracing::info!(id, "dismissing notification");
                        close_notification(id, 2, &mut notifications, &mut history, history_limit, &iface_ref).await;
                        emit(&notifications, &history, dnd, &broker_tx);
                        let _ = reply.send(Ok(serde_json::json!(null)));
                    }
                    NotifyMessage::DismissAll { reply } => {
                        let ids: Vec<u32> = notifications.keys().copied().collect();
                        tracing::info!(count = ids.len(), "dismissing all notifications");
                        for id in ids {
                            close_notification(id, 2, &mut notifications, &mut history, history_limit, &iface_ref).await;
                        }
                        emit(&notifications, &history, dnd, &broker_tx);
                        let _ = reply.send(Ok(serde_json::json!(null)));
                    }
                    NotifyMessage::InvokeAction { id, action_key, reply } => {
                        tracing::info!(id, action_key, "invoking action");
                        let signal_emitter = iface_ref.signal_emitter();
                        let _ = NotificationServer::action_invoked(signal_emitter, id, &action_key).await;
                        let resident = notifications.get(&id)
                            .and_then(|n| n["resident"].as_bool())
                            .unwrap_or(false);
                        if !resident {
                            close_notification(id, 2, &mut notifications, &mut history, history_limit, &iface_ref).await;
                        }
                        emit(&notifications, &history, dnd, &broker_tx);
                        let _ = reply.send(Ok(serde_json::json!(null)));
                    }
                    NotifyMessage::SetDnd { enabled, reply } => {
                        tracing::info!(enabled, "setting DnD");
                        dnd = enabled;
                        emit(&notifications, &history, dnd, &broker_tx);
                        let _ = reply.send(Ok(serde_json::json!(null)));
                    }
                    NotifyMessage::ClearHistory { reply } => {
                        tracing::info!("clearing history");
                        history.clear();
                        emit(&notifications, &history, dnd, &broker_tx);
                        let _ = reply.send(Ok(serde_json::json!(null)));
                    }
                }
            }
            () = &mut expiry => {
                let now = glimpse_types::now_ms();
                let expired: Vec<u32> = notifications.iter()
                    .filter(|(_, n)| {
                        let t = n["expire_timeout"].as_i64().unwrap_or(0);
                        t > 0 && now >= n["timestamp"].as_u64().unwrap_or(0) + t as u64
                    })
                    .map(|(id, _)| *id)
                    .collect();

                for id in &expired {
                    if let Some(notif) = notifications.remove(id) {
                        tracing::info!(id, "notification expired");
                        history.push_front(serde_json::json!({
                            "id": id,
                            "app_name": notif["app_name"],
                            "app_icon": notif["app_icon"],
                            "summary": notif["summary"],
                            "body": notif["body"],
                            "urgency": notif["urgency"],
                            "timestamp": notif["timestamp"],
                            "close_reason": 1,
                        }));
                        while history.len() > history_limit { history.pop_back(); }

                        let signal_emitter = iface_ref.signal_emitter();
                        let _ = NotificationServer::notification_closed(signal_emitter, *id, 1).await;
                    }
                }

                if !expired.is_empty() {
                    emit(&notifications, &history, dnd, &broker_tx);
                }

                // Reset timer
                let next_expiry = notifications.values()
                    .filter_map(|n| {
                        let t = n["expire_timeout"].as_i64().unwrap_or(0);
                        let now = glimpse_types::now_ms();
                        if t > 0 {
                            let deadline = n["timestamp"].as_u64().unwrap_or(0) + t as u64;
                            Some(deadline.saturating_sub(now))
                        } else { None }
                    })
                    .min()
                    .unwrap_or(86400_000);
                expiry.as_mut().reset(tokio::time::Instant::now() + std::time::Duration::from_millis(next_expiry.max(10)));
            }
        }
    }

    let _ = conn.release_name("org.freedesktop.Notifications").await;
    tracing::info!("notification-server: stopped");
    Ok(())
}
