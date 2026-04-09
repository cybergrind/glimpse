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

    pub async fn lock(&self) -> anyhow::Result<()> {
        let conn = self.connection().await?;
        Login1ManagerProxy::new(&conn)
            .await?
            .lock_sessions()
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

#[cfg(test)]
mod tests {
    use super::*;

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
