use std::collections::HashMap;

use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::OwnedValue;

use crate::dbus::DbusPropertyGroup;

#[derive(Debug, Clone, Serialize, Default)]
pub struct PowerProfiles {
    pub active: String,
    pub available: Vec<String>,
    pub performance_degraded: String,
}

#[derive(Debug, Clone, Serialize, Default)]
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
    profiles: PowerProfiles,
    actions: PowerActions,
}

impl PowerProvider {
    pub fn new() -> Self {
        Self {
            profiles: PowerProfiles::default(),
            actions: PowerActions::default(),
        }
    }

    pub async fn run(
        &mut self,
        conn: zbus::Connection,
        events: mpsc::Sender<PowerEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let logind = DbusPropertyGroup::new(
            &conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;

        self.actions = PowerActions {
            can_suspend: logind.call("CanSuspend", &()).await.unwrap_or_default(),
            can_hibernate: logind.call("CanHibernate", &()).await.unwrap_or_default(),
            can_reboot: logind.call("CanReboot", &()).await.unwrap_or_default(),
            can_poweroff: logind.call("CanPowerOff", &()).await.unwrap_or_default(),
        };
        let _ = events
            .send(PowerEvent::ActionsChanged(self.actions.clone()))
            .await;

        let profiles_proxy = DbusPropertyGroup::new(
            &conn,
            "net.hadess.PowerProfiles",
            "/net/hadess/PowerProfiles",
            "net.hadess.PowerProfiles",
        )
        .await;

        if let Ok(ref pp) = profiles_proxy {
            read_profiles(&mut self.profiles, pp).await;
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

        let mut profile_changes = match &profiles_proxy {
            Ok(pp) => Some(pp.stream_changes().await?),
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
                    if let Ok(ref pp) = profiles_proxy {
                        read_profiles(&mut self.profiles, pp).await;
                        if events.send(PowerEvent::ProfilesChanged(self.profiles.clone())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

async fn read_profiles(profiles: &mut PowerProfiles, pp: &DbusPropertyGroup) {
    profiles.active = pp.get("ActiveProfile").await.unwrap_or_default();
    profiles.performance_degraded = pp.get("PerformanceDegraded").await.unwrap_or_default();

    let raw: Vec<HashMap<String, OwnedValue>> = pp.get("Profiles").await.unwrap_or_default();
    profiles.available = raw
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
        .collect();
}

pub async fn set_profile(conn: &zbus::Connection, profile: &str) -> anyhow::Result<()> {
    let pp = DbusPropertyGroup::new(
        conn,
        "net.hadess.PowerProfiles",
        "/net/hadess/PowerProfiles",
        "net.hadess.PowerProfiles",
    )
    .await?;
    pp.set("ActiveProfile", profile.to_owned())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

pub async fn suspend(conn: &zbus::Connection) -> anyhow::Result<()> {
    logind_action(conn, "Suspend").await
}

pub async fn hibernate(conn: &zbus::Connection) -> anyhow::Result<()> {
    logind_action(conn, "Hibernate").await
}

pub async fn reboot(conn: &zbus::Connection) -> anyhow::Result<()> {
    logind_action(conn, "Reboot").await
}

pub async fn poweroff(conn: &zbus::Connection) -> anyhow::Result<()> {
    logind_action(conn, "PowerOff").await
}

pub async fn lock(conn: &zbus::Connection) -> anyhow::Result<()> {
    let logind = DbusPropertyGroup::new(
        conn,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    logind
        .call_void("LockSessions", &())
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

async fn logind_action(conn: &zbus::Connection, method: &str) -> anyhow::Result<()> {
    let logind = DbusPropertyGroup::new(
        conn,
        "org.freedesktop.login1",
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
    )
    .await
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    logind
        .call_void(method, &(false,))
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}
