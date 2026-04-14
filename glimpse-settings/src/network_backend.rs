use anyhow::Context;
use glimpse::network::{
    NetworkServiceHandle,
    protocol::NetworkServiceState,
    provider::{HotspotConfig, NetworkConnectionConfig, NetworkProvider},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub enum NetworkBackendEvent {
    ServiceState(NetworkServiceState),
    Unavailable(String),
}

#[derive(Clone)]
pub struct NetworkBackend {
    service: NetworkServiceHandle,
}

impl NetworkBackend {
    pub fn new(service: NetworkServiceHandle) -> Self {
        Self { service }
    }

    pub fn service(&self) -> &NetworkServiceHandle {
        &self.service
    }

    pub async fn run(
        &self,
        events: mpsc::Sender<NetworkBackendEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let mut state_rx = self.service.subscribe();
        let initial_state = state_rx.borrow().clone();
        let _ = events
            .send(NetworkBackendEvent::ServiceState(initial_state))
            .await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                result = state_rx.changed() => {
                    if result.is_err() {
                        let _ = events
                            .send(NetworkBackendEvent::Unavailable("Network service unavailable".into()))
                            .await;
                        break;
                    }
                    let state = state_rx.borrow().clone();
                    if events.send(NetworkBackendEvent::ServiceState(state)).await.is_err() {
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn load_connection_config(&self, uuid: &str) -> anyhow::Result<NetworkConnectionConfig> {
        let provider = self.provider().await?;
        provider.load_connection_config(uuid).await
    }

    pub async fn apply_connection_config(&self, config: &NetworkConnectionConfig) -> anyhow::Result<()> {
        let provider = self.provider().await?;
        provider.apply_connection_config(config).await
    }

    pub async fn load_hotspot_config(&self, device_path: &str) -> anyhow::Result<HotspotConfig> {
        let provider = self.provider().await?;
        provider.load_hotspot_config(device_path).await
    }

    pub async fn apply_hotspot_config(&self, config: &HotspotConfig) -> anyhow::Result<HotspotConfig> {
        let provider = self.provider().await?;
        provider.apply_hotspot_config(config).await
    }

    pub async fn set_hotspot_enabled(&self, config: &HotspotConfig, enabled: bool) -> anyhow::Result<()> {
        let provider = self.provider().await?;
        provider.set_hotspot_enabled(config, enabled).await
    }

    async fn provider(&self) -> anyhow::Result<NetworkProvider> {
        let system = zbus::Connection::system()
            .await
            .context("system bus unavailable for network settings")?;
        Ok(NetworkProvider::new(system))
    }
}
