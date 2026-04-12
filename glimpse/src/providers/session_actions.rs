use serde::Serialize;

use crate::dbus::login1::Login1ManagerProxy;

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionActionAvailability {
    Available,
    Challenge,
    #[default]
    Unavailable,
    Unknown(String),
}

impl SessionActionAvailability {
    pub fn from_login1(value: &str) -> Self {
        match value {
            "yes" => Self::Available,
            "challenge" => Self::Challenge,
            "no" | "na" => Self::Unavailable,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionBackendState {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionActionCapabilities {
    pub backend: SessionBackendState,
    pub suspend: SessionActionAvailability,
    pub hibernate: SessionActionAvailability,
    pub reboot: SessionActionAvailability,
    pub power_off: SessionActionAvailability,
    pub lock: SessionActionAvailability,
}

impl Default for SessionActionCapabilities {
    fn default() -> Self {
        Self::unavailable()
    }
}

impl SessionActionCapabilities {
    fn unavailable() -> Self {
        Self {
            backend: SessionBackendState::Unavailable,
            suspend: SessionActionAvailability::Unavailable,
            hibernate: SessionActionAvailability::Unavailable,
            reboot: SessionActionAvailability::Unavailable,
            power_off: SessionActionAvailability::Unavailable,
            lock: SessionActionAvailability::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub capabilities: SessionActionCapabilities,
    pub user_name: String,
    pub host_name: String,
    pub subtitle: String,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            capabilities: SessionActionCapabilities::default(),
            user_name: "user".into(),
            host_name: "linux".into(),
            subtitle: String::new(),
        }
    }
}

#[derive(Clone)]
enum SessionConnectionSource {
    System,
    Provided(zbus::Connection),
}

pub struct SessionActions {
    connection: SessionConnectionSource,
}

impl SessionActions {
    pub fn new() -> Self {
        Self {
            connection: SessionConnectionSource::System,
        }
    }

    pub fn with_connection(conn: zbus::Connection) -> Self {
        Self {
            connection: SessionConnectionSource::Provided(conn),
        }
    }

    pub async fn capabilities(&self) -> anyhow::Result<SessionActionCapabilities> {
        let conn = match self.connection().await {
            Ok(conn) => conn,
            Err(error) => {
                tracing::warn!(error = %error, "session actions: login1 backend unavailable");
                return Ok(SessionActionCapabilities::unavailable());
            }
        };

        let manager = match Login1ManagerProxy::new(&conn).await {
            Ok(manager) => manager,
            Err(error) => {
                tracing::warn!(error = %error, "session actions: failed to create login1 proxy");
                return Ok(SessionActionCapabilities::unavailable());
            }
        };

        Ok(SessionActionCapabilities {
            backend: SessionBackendState::Available,
            suspend: capability_from_result(manager.can_suspend().await),
            hibernate: capability_from_result(manager.can_hibernate().await),
            reboot: capability_from_result(manager.can_reboot().await),
            power_off: capability_from_result(manager.can_power_off().await),
            lock: SessionActionAvailability::Available,
        })
    }

    pub async fn snapshot(&self) -> anyhow::Result<SessionSnapshot> {
        let host_name = read_hostname();
        let uptime = read_uptime().map(format_uptime).unwrap_or_default();

        Ok(SessionSnapshot {
            capabilities: self.capabilities().await?,
            user_name: std::env::var("USER").unwrap_or_else(|_| "user".into()),
            subtitle: build_subtitle(&host_name, &uptime),
            host_name,
        })
    }

    pub async fn lock(&self) -> anyhow::Result<()> {
        let conn = self.connection().await?;
        Login1ManagerProxy::new(&conn)
            .await?
            .lock_sessions()
            .await
            .map_err(Into::into)
    }

    pub async fn logout(&self) -> anyhow::Result<()> {
        let session_id = resolve_logout_session_id(|key| std::env::var(key)).ok_or_else(|| {
            anyhow::anyhow!("session actions: XDG_SESSION_ID is required for logout")
        })?;
        let conn = self.connection().await?;
        Login1ManagerProxy::new(&conn)
            .await?
            .terminate_session(&session_id)
            .await
            .map_err(Into::into)
    }

    pub async fn suspend(&self) -> anyhow::Result<()> {
        let conn = self.connection().await?;
        Login1ManagerProxy::new(&conn)
            .await?
            .suspend(false)
            .await
            .map_err(Into::into)
    }

    pub async fn hibernate(&self) -> anyhow::Result<()> {
        let conn = self.connection().await?;
        Login1ManagerProxy::new(&conn)
            .await?
            .hibernate(false)
            .await
            .map_err(Into::into)
    }

    pub async fn reboot(&self) -> anyhow::Result<()> {
        let conn = self.connection().await?;
        Login1ManagerProxy::new(&conn)
            .await?
            .reboot(false)
            .await
            .map_err(Into::into)
    }

    pub async fn power_off(&self) -> anyhow::Result<()> {
        let conn = self.connection().await?;
        Login1ManagerProxy::new(&conn)
            .await?
            .power_off(false)
            .await
            .map_err(Into::into)
    }

    async fn connection(&self) -> anyhow::Result<zbus::Connection> {
        match &self.connection {
            SessionConnectionSource::System => zbus::Connection::system().await.map_err(Into::into),
            SessionConnectionSource::Provided(conn) => Ok(conn.clone()),
        }
    }
}

fn capability_from_result(result: zbus::Result<String>) -> SessionActionAvailability {
    match result {
        Ok(value) => SessionActionAvailability::from_login1(value.as_str()),
        Err(error) => {
            tracing::warn!(error = %error, "session actions: capability query failed");
            SessionActionAvailability::Unavailable
        }
    }
}

fn resolve_logout_session_id<F>(env: F) -> Option<String>
where
    F: for<'a> Fn(&'a str) -> Result<String, std::env::VarError>,
{
    env("XDG_SESSION_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn read_hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|value| value.trim().to_owned())
        .unwrap_or_else(|_| "linux".into())
}

fn read_uptime() -> Option<u64> {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|value| value.split_whitespace().next()?.parse::<f64>().ok())
        .map(|seconds| seconds as u64)
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let mins = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

fn build_subtitle(host_name: &str, uptime: &str) -> String {
    if uptime.is_empty() {
        host_name.to_owned()
    } else {
        format!("{host_name} · up {uptime}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_snapshot_default_is_unavailable() {
        let snapshot = SessionSnapshot::default();

        assert_eq!(
            snapshot.capabilities.backend,
            SessionBackendState::Unavailable
        );
        assert_eq!(snapshot.user_name, "user");
        assert_eq!(snapshot.host_name, "linux");
        assert!(snapshot.subtitle.is_empty());
    }

    #[test]
    fn format_uptime_displays_days_hours_and_minutes() {
        assert_eq!(format_uptime(65), "1m");
        assert_eq!(format_uptime(3720), "1h 2m");
        assert_eq!(format_uptime(90061), "1d 1h");
    }

    #[test]
    fn build_subtitle_uses_hostname_when_uptime_missing() {
        assert_eq!(build_subtitle("workstation", ""), "workstation");
        assert_eq!(
            build_subtitle("workstation", "2h 5m"),
            "workstation · up 2h 5m"
        );
    }

    #[test]
    fn resolve_logout_session_id_prefers_explicit_session() {
        let env = [("XDG_SESSION_ID", "c2"), ("USER", "alex")];
        assert_eq!(
            resolve_logout_session_id(|key| {
                env.iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| (*value).to_string())
                    .ok_or(std::env::VarError::NotPresent)
            })
            .as_deref(),
            Some("c2")
        );
    }

    #[test]
    fn session_action_availability_maps_login1_values() {
        assert_eq!(
            SessionActionAvailability::from_login1("yes"),
            SessionActionAvailability::Available
        );
        assert_eq!(
            SessionActionAvailability::from_login1("challenge"),
            SessionActionAvailability::Challenge
        );
        assert_eq!(
            SessionActionAvailability::from_login1("no"),
            SessionActionAvailability::Unavailable
        );
        assert_eq!(
            SessionActionAvailability::from_login1("na"),
            SessionActionAvailability::Unavailable
        );
    }

    #[test]
    fn session_action_availability_preserves_unknown_values() {
        assert_eq!(
            SessionActionAvailability::from_login1("limited"),
            SessionActionAvailability::Unknown("limited".into())
        );
    }

    #[test]
    fn failed_capability_query_degrades_to_unavailable() {
        assert_eq!(
            capability_from_result(Err(zbus::Error::Failure("boom".into()))),
            SessionActionAvailability::Unavailable
        );
    }

    #[test]
    fn unavailable_capabilities_disable_all_actions() {
        let caps = SessionActionCapabilities::default();

        assert_eq!(caps.backend, SessionBackendState::Unavailable);
        assert_eq!(caps.suspend, SessionActionAvailability::Unavailable);
        assert_eq!(caps.hibernate, SessionActionAvailability::Unavailable);
        assert_eq!(caps.reboot, SessionActionAvailability::Unavailable);
        assert_eq!(caps.power_off, SessionActionAvailability::Unavailable);
        assert_eq!(caps.lock, SessionActionAvailability::Unavailable);
    }
}
