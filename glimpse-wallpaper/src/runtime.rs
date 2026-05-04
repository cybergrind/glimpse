use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use anyhow::{Context, anyhow, bail};
use glimpse_config::ResolvedWallpaperSpec;
use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};

pub const APP_ID: &str = "me.aresa.GlimpseWallpaper";
pub const GTK_APPLICATION_ID: &str = "me.aresa.GlimpseWallpaper.App";
pub const WALLPAPER_NAMESPACE: &str = "glimpse-wallpaper";
pub const BACKDROP_NAMESPACE: &str = "glimpse-backdrop";

#[derive(Debug, Default)]
pub struct WallpaperRuntime {
    next_request: u64,
    active_request: Option<u64>,
    active_image_path: Option<PathBuf>,
}

impl WallpaperRuntime {
    pub fn begin_image_load(&mut self, spec: ResolvedWallpaperSpec) -> u64 {
        self.next_request += 1;
        let request = self.next_request;
        self.active_request = Some(request);
        if spec.image.is_none() {
            self.active_image_path = None;
        }
        request
    }

    pub fn finish_image_load(&mut self, result: ImageLoadResult) -> bool {
        if Some(result.request_id) != self.active_request {
            return false;
        }

        match result.path {
            Some(path) => {
                self.active_image_path = Some(path);
                true
            }
            None => false,
        }
    }

    pub fn active_image_path(&self) -> Option<PathBuf> {
        self.active_image_path.clone()
    }

    pub async fn acquire_single_instance() -> anyhow::Result<InstanceGuard> {
        acquire_dbus_name(APP_ID).await
    }

    pub async fn acquire_single_instance_for_testing(
        name: &str,
    ) -> anyhow::Result<TestInstanceGuard> {
        TestInstanceGuard::acquire(name)
    }
}

#[derive(Debug, Clone)]
pub struct ImageLoadResult {
    request_id: u64,
    path: Option<PathBuf>,
}

impl ImageLoadResult {
    pub fn loaded(request_id: u64, path: PathBuf) -> Self {
        Self {
            request_id,
            path: Some(path),
        }
    }

    pub fn failed(request_id: u64) -> Self {
        Self {
            request_id,
            path: None,
        }
    }
}

pub struct InstanceGuard {
    _name: &'static str,
    _connection: zbus::Connection,
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
