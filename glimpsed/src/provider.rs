use std::future::Future;
use std::pin::Pin;

use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// Event emitted by a running provider.
pub struct ProviderEvent {
    pub topic: String,
    pub data: serde_json::Value,
}

/// Request sent from the broker to a running provider.
pub enum ProviderRequest {
    Snapshot {
        topic: String,
        reply: oneshot::Sender<Option<serde_json::Value>>,
    },
    Call {
        method: String,
        params: serde_json::Value,
        reply: oneshot::Sender<anyhow::Result<serde_json::Value>>,
    },
}

/// Object-safe system service provider.
pub trait Provider: Send + 'static {
    fn name(&self) -> &'static str;
    fn topics(&self) -> &'static [&'static str];
    fn methods(&self) -> &'static [&'static str];

    /// Run the provider event loop. Receive requests via `requests`, emit events via `events`.
    /// Return when `cancel` fires or on fatal error.
    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + '_>>;
}

/// Creates provider instances (allows restart after crash).
pub trait ProviderFactory: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn topics(&self) -> &'static [&'static str];
    fn methods(&self) -> &'static [&'static str];
    fn create(&self) -> Box<dyn Provider>;
}
