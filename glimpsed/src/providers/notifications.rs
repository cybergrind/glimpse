use std::pin::Pin;

use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::notification_server::NotifyMessage;
use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "notifications";
const TOPICS: &[&str] = &[
    "notifications.status",
    "notifications.list",
    "notifications.history",
];
const METHODS: &[&str] = &[
    "notifications.dismiss",
    "notifications.dismiss_all",
    "notifications.invoke_action",
    "notifications.set_dnd",
    "notifications.clear_history",
];

/// The notifications provider is a thin method forwarder.
/// All state lives in the standalone notification_server task.
/// This provider exists so the broker can route method calls (dismiss, etc.)
/// from panel clients to the notification server.
pub struct NotificationsProvider {
    server_tx: mpsc::Sender<NotifyMessage>,
}

impl Provider for NotificationsProvider {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }

    fn run(
        &mut self,
        _events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("notifications provider: started (method forwarder)");

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req).await;
                    }
                }
            }
            Ok(())
        })
    }
}

impl NotificationsProvider {
    async fn handle_request(&self, req: ProviderRequest) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                // Snapshots are handled by the broker's event cache from
                // the notification server's direct ProviderEvent emissions.
                // Return None here — the broker will use its cached data.
                let _ = reply.send(None);
            }
            ProviderRequest::Call { method, params, reply } => {
                let result = match method.as_str() {
                    "notifications.dismiss" => {
                        let Some(id) = params["id"].as_u64() else {
                            let _ = reply.send(Err(anyhow::anyhow!("missing 'id'")));
                            return;
                        };
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let _ = self.server_tx.send(NotifyMessage::Dismiss {
                            id: id as u32, reply: tx,
                        }).await;
                        rx.await.unwrap_or(Err(anyhow::anyhow!("server gone")))
                    }
                    "notifications.dismiss_all" => {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let _ = self.server_tx.send(NotifyMessage::DismissAll { reply: tx }).await;
                        rx.await.unwrap_or(Err(anyhow::anyhow!("server gone")))
                    }
                    "notifications.invoke_action" => {
                        let Some(id) = params["id"].as_u64() else {
                            let _ = reply.send(Err(anyhow::anyhow!("missing 'id'")));
                            return;
                        };
                        let Some(action_key) = params["action_key"].as_str() else {
                            let _ = reply.send(Err(anyhow::anyhow!("missing 'action_key'")));
                            return;
                        };
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let _ = self.server_tx.send(NotifyMessage::InvokeAction {
                            id: id as u32, action_key: action_key.to_owned(), reply: tx,
                        }).await;
                        rx.await.unwrap_or(Err(anyhow::anyhow!("server gone")))
                    }
                    "notifications.set_dnd" => {
                        let Some(enabled) = params["enabled"].as_bool() else {
                            let _ = reply.send(Err(anyhow::anyhow!("missing 'enabled'")));
                            return;
                        };
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let _ = self.server_tx.send(NotifyMessage::SetDnd {
                            enabled, reply: tx,
                        }).await;
                        rx.await.unwrap_or(Err(anyhow::anyhow!("server gone")))
                    }
                    "notifications.clear_history" => {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let _ = self.server_tx.send(NotifyMessage::ClearHistory { reply: tx }).await;
                        rx.await.unwrap_or(Err(anyhow::anyhow!("server gone")))
                    }
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                if let Err(ref e) = result {
                    tracing::warn!(method = %method, error = %e, "notifications: call failed");
                }
                let _ = reply.send(result);
            }
        }
    }
}

pub struct NotificationsProviderFactory {
    pub server_tx: mpsc::Sender<NotifyMessage>,
}

impl ProviderFactory for NotificationsProviderFactory {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(NotificationsProvider {
            server_tx: self.server_tx.clone(),
        })
    }
}
