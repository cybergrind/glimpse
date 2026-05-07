use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, anyhow, bail};
use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};

pub const APP_ID: &str = "me.aresa.GlimpseWallpaper";
pub const GTK_APPLICATION_ID: &str = "me.aresa.GlimpseWallpaper.App";
pub const WALLPAPER_NAMESPACE: &str = "glimpse-wallpaper";
pub const BACKDROP_NAMESPACE: &str = "glimpse-backdrop";

#[derive(Debug, Default)]
pub struct WallpaperRuntime;

impl WallpaperRuntime {
    pub async fn acquire_single_instance() -> anyhow::Result<InstanceGuard> {
        acquire_dbus_name(APP_ID).await
    }

    pub async fn acquire_single_instance_with_name(
        name: impl Into<String>,
    ) -> anyhow::Result<InstanceGuard> {
        acquire_dbus_name(name.into()).await
    }

    pub async fn acquire_single_instance_for_testing(
        name: &str,
    ) -> anyhow::Result<TestInstanceGuard> {
        TestInstanceGuard::acquire(name)
    }
}

pub struct InstanceGuard {
    _name: String,
    _connection: zbus::Connection,
}

async fn acquire_dbus_name(name: impl Into<String>) -> anyhow::Result<InstanceGuard> {
    let name = name.into();
    tracing::debug!(name, "connecting to session D-Bus");
    let connection = zbus::Connection::session()
        .await
        .context("connect to session D-Bus")?;
    let proxy = DBusProxy::new(&connection)
        .await
        .context("create session D-Bus proxy")?;
    let well_known_name = zbus::names::WellKnownName::try_from(name.as_str())
        .with_context(|| format!("validate D-Bus name {name}"))?;
    let reply = proxy
        .request_name(well_known_name, RequestNameFlags::DoNotQueue.into())
        .await
        .with_context(|| format!("request D-Bus name {name}"))?;

    match reply {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {
            tracing::debug!(name, reply = ?reply, "D-Bus name acquired");
            Ok(InstanceGuard {
                _name: name,
                _connection: connection,
            })
        }
        RequestNameReply::Exists | RequestNameReply::InQueue => {
            bail!("another glimpse-wallpaper instance already owns {name}")
        }
    }
}

#[derive(Debug)]
pub struct TestInstanceGuard {
    name: String,
}

impl TestInstanceGuard {
    fn acquire(name: &str) -> anyhow::Result<Self> {
        let mut names = test_names()
            .lock()
            .map_err(|_| anyhow!("test instance registry is poisoned"))?;
        if names.contains(name) {
            bail!("another glimpse-wallpaper instance already owns {name}");
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
    use super::{APP_ID, GTK_APPLICATION_ID};

    #[test]
    fn gtk_application_id_does_not_reuse_single_instance_bus_name() {
        assert_ne!(GTK_APPLICATION_ID, APP_ID);
    }
}
