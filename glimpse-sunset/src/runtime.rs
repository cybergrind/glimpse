use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, anyhow, bail};
use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};

pub const APP_ID: &str = "me.aresa.GlimpseSunset";

pub struct InstanceGuard {
    _connection: zbus::Connection,
}

pub async fn acquire_single_instance() -> anyhow::Result<InstanceGuard> {
    acquire_dbus_name(APP_ID).await
}

async fn acquire_dbus_name(name: &'static str) -> anyhow::Result<InstanceGuard> {
    tracing::debug!(name, "connecting to session D-Bus");
    let connection = zbus::Connection::session()
        .await
        .context("connect to session D-Bus")?;
    let proxy = DBusProxy::new(&connection)
        .await
        .context("create session D-Bus proxy")?;
    let well_known_name = zbus::names::WellKnownName::try_from(name)
        .with_context(|| format!("validate D-Bus name {name}"))?;
    let reply = proxy
        .request_name(well_known_name, RequestNameFlags::DoNotQueue.into())
        .await
        .with_context(|| format!("request D-Bus name {name}"))?;

    match reply {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {
            tracing::debug!(name, reply = ?reply, "D-Bus name acquired");
            Ok(InstanceGuard {
                _connection: connection,
            })
        }
        RequestNameReply::Exists | RequestNameReply::InQueue => {
            bail!("another glimpse-sunset instance already owns {name}")
        }
    }
}

#[derive(Debug)]
pub struct TestInstanceGuard {
    name: String,
}

impl TestInstanceGuard {
    pub fn acquire(name: &str) -> anyhow::Result<Self> {
        let mut names = test_names()
            .lock()
            .map_err(|_| anyhow!("test instance registry is poisoned"))?;
        if names.contains(name) {
            bail!("another glimpse-sunset instance already owns {name}");
        }
        names.insert(name.to_string());
        Ok(Self { name: name.into() })
    }
}

impl Drop for TestInstanceGuard {
    fn drop(&mut self) {
        if let Ok(mut names) = test_names().lock() {
            names.remove(&self.name);
        }
    }
}

fn test_names() -> &'static Mutex<HashSet<String>> {
    static NAMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    NAMES.get_or_init(|| Mutex::new(HashSet::new()))
}

#[cfg(test)]
mod tests {
    use super::{APP_ID, TestInstanceGuard};

    #[test]
    fn app_id_is_stable() {
        assert_eq!(APP_ID, "me.aresa.GlimpseSunset");
    }

    #[test]
    fn test_instance_guard_rejects_second_owner() {
        let _first = TestInstanceGuard::acquire("me.aresa.GlimpseSunset.Test").expect("first");
        let second = TestInstanceGuard::acquire("me.aresa.GlimpseSunset.Test");

        assert!(second.is_err());
    }
}
