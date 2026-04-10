use glimpse::providers::{
    battery::{BatteryEvent, BatteryProvider},
    power::{PowerEvent, PowerProvider},
    power_policy::{PowerPolicyEvent, PowerPolicySettings, PowerPolicySnapshot},
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::power::PowerDraft;

#[derive(Debug, Clone)]
pub enum PowerBackendEvent {
    Battery(BatteryEvent),
    Power(PowerEvent),
    Policy(PowerPolicyEvent),
}

#[derive(Clone, Default)]
pub struct PowerBackend;

impl PowerBackend {
    pub fn new() -> Self {
        Self
    }

    pub fn initial_draft(&self) -> PowerDraft {
        let policy = self.load_policy();
        PowerDraft {
            profile: String::new(),
            policy,
        }
    }

    pub fn load_policy(&self) -> PowerPolicySnapshot {
        PowerPolicySettings::new().load().unwrap_or_default()
    }

    pub async fn run(
        &self,
        events: mpsc::Sender<PowerBackendEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let (battery_tx, mut battery_rx) = mpsc::channel(8);
        let (power_tx, mut power_rx) = mpsc::channel(8);

        match zbus::Connection::system().await {
            Ok(system) => {
                let battery_cancel = cancel.child_token();
                let mut battery = BatteryProvider::new(system.clone());
                tokio::spawn(async move {
                    if let Err(error) = battery.run(battery_tx, battery_cancel).await {
                        tracing::warn!("power settings battery backend failed: {error}");
                    }
                });

                let power_cancel = cancel.child_token();
                let mut power = PowerProvider::new(system);
                tokio::spawn(async move {
                    if let Err(error) = power.run(power_tx, power_cancel).await {
                        tracing::warn!("power settings power backend failed: {error}");
                    }
                });
            }
            Err(error) => {
                tracing::warn!("power settings system bus unavailable: {error}");
            }
        }

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(event) = battery_rx.recv() => {
                    if events.send(PowerBackendEvent::Battery(event)).await.is_err() {
                        break;
                    }
                }
                Some(event) = power_rx.recv() => {
                    if events.send(PowerBackendEvent::Power(event)).await.is_err() {
                        break;
                    }
                }
                else => break,
            }
        }

        Ok(())
    }

    pub fn apply_policy(&self, draft: &PowerDraft) -> anyhow::Result<()> {
        PowerPolicySettings::new().apply(&draft.policy)
    }

    pub async fn apply_profile(&self, profile: &str) -> anyhow::Result<()> {
        if profile.is_empty() {
            return Ok(());
        }

        let system = zbus::Connection::system().await?;
        PowerProvider::new(system).set_profile(profile).await
    }
}
