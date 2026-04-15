use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::dbus::login1::Login1ManagerProxy;
use crate::dbus::power_profiles::PowerProfilesDaemonProxy;

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct PowerProfiles {
    pub active: String,
    pub available: Vec<String>,
    pub performance_degraded: String,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct PowerActions {
    pub can_suspend: String,
    pub can_hibernate: String,
    pub can_reboot: String,
    pub can_poweroff: String,
}

#[derive(Debug, Clone)]
pub enum PowerEvent {
    ProfilesChanged(PowerProfiles),
    ActionsChanged(PowerActions),
}

pub struct PowerProvider {
    conn: zbus::Connection,
    profiles: PowerProfiles,
    actions: PowerActions,
}

impl PowerProvider {
    pub fn new(conn: zbus::Connection) -> Self {
        Self {
            conn,
            profiles: PowerProfiles::default(),
            actions: PowerActions::default(),
        }
    }

    pub async fn run(
        &mut self,
        events: mpsc::Sender<PowerEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let logind = Login1ManagerProxy::new(&self.conn).await?;

        self.actions = PowerActions {
            can_suspend: logind.can_suspend().await.unwrap_or_default(),
            can_hibernate: logind.can_hibernate().await.unwrap_or_default(),
            can_reboot: logind.can_reboot().await.unwrap_or_default(),
            can_poweroff: logind.can_power_off().await.unwrap_or_default(),
        };
        let _ = events
            .send(PowerEvent::ActionsChanged(self.actions.clone()))
            .await;

        let pp = PowerProfilesDaemonProxy::new(&self.conn).await;

        if let Ok(ref pp) = pp {
            self.read_profiles(pp).await;
            tracing::info!(
                active = %self.profiles.active,
                available = ?self.profiles.available,
                "power: profiles loaded"
            );
            let _ = events
                .send(PowerEvent::ProfilesChanged(self.profiles.clone()))
                .await;
        } else {
            tracing::warn!("power: power-profiles-daemon not available");
        }

        let mut profile_changes = match &pp {
            Ok(pp) => Some(pp.receive_active_profile_changed().await),
            Err(_) => None,
        };

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(_) = async {
                    match &mut profile_changes {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Ok(ref pp) = pp {
                        if self.read_profiles(pp).await
                            && events.send(PowerEvent::ProfilesChanged(self.profiles.clone())).await.is_err()
                        {
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn set_profile(&self, profile: &str) -> anyhow::Result<()> {
        let pp = PowerProfilesDaemonProxy::new(&self.conn).await?;
        pp.set_active_profile(profile)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn suspend(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .suspend(false)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn hibernate(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .hibernate(false)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn reboot(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .reboot(false)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn poweroff(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .power_off(false)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub async fn lock(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .lock_sessions()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn logind(&self) -> anyhow::Result<Login1ManagerProxy<'_>> {
        Login1ManagerProxy::new(&self.conn)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn read_profiles(&mut self, pp: &PowerProfilesDaemonProxy<'_>) -> bool {
        let raw = pp.profiles().await.unwrap_or_default();
        let next = PowerProfiles {
            active: pp.active_profile().await.unwrap_or_default(),
            performance_degraded: pp.performance_degraded().await.unwrap_or_default(),
            available: raw
                .iter()
                .filter_map(|d| {
                    d.get("Profile").and_then(|v| {
                        use zbus::zvariant::Value;
                        match &**v {
                            Value::Str(s) => Some(s.to_string()),
                            _ => None,
                        }
                    })
                })
                .collect(),
        };

        let changed = should_emit_profiles(&self.profiles, &next);
        self.profiles = next;
        changed
    }
}

fn should_emit_profiles(previous: &PowerProfiles, next: &PowerProfiles) -> bool {
    previous != next
}

#[cfg(test)]
mod tests {
    use super::PowerProfiles;

    #[test]
    fn should_emit_profiles_only_for_real_changes() {
        let previous = PowerProfiles {
            active: "balanced".into(),
            available: vec![
                "power-saver".into(),
                "balanced".into(),
                "performance".into(),
            ],
            performance_degraded: String::new(),
        };

        assert!(!super::should_emit_profiles(&previous, &previous));

        let mut next = previous.clone();
        next.active = "performance".into();
        assert!(super::should_emit_profiles(&previous, &next));
    }
}
