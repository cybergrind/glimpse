use std::sync::Arc;

use async_trait::async_trait;
use zbus::zvariant::OwnedObjectPath;

use crate::dbus::geoclue::{GeoClueClientProxy, GeoClueLocationProxy, GeoClueManagerProxy};

const GEOCLUE_DESKTOP_ID: &str = "me.aresa.GlimpsePanel";
const GEOCLUE_ACCURACY_LEVEL_CITY: u32 = 4;
const GEOCLUE_LOCATION_POLL_ATTEMPTS: usize = 10;
const GEOCLUE_LOCATION_POLL_DELAY_MS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordinatesPair {
    pub latitude: f64,
    pub longitude: f64,
}

#[async_trait]
pub trait LocationSource: Send + Sync {
    async fn resolve_coordinates(&self) -> anyhow::Result<CoordinatesPair>;
}

#[derive(Clone)]
pub struct LocationProvider {
    source: Arc<dyn LocationSource>,
}

impl Default for LocationProvider {
    fn default() -> Self {
        Self::with_source(Arc::new(GeoClueLocationSource))
    }
}

impl LocationProvider {
    pub fn with_source(source: Arc<dyn LocationSource>) -> Self {
        Self { source }
    }

    pub async fn resolve_coordinates(&self) -> anyhow::Result<CoordinatesPair> {
        let coordinates = self.source.resolve_coordinates().await?;
        tracing::info!(
            latitude = coordinates.latitude,
            longitude = coordinates.longitude,
            "location provider: resolved coordinates"
        );
        Ok(coordinates)
    }
}

struct GeoClueLocationSource;

#[async_trait]
impl LocationSource for GeoClueLocationSource {
    async fn resolve_coordinates(&self) -> anyhow::Result<CoordinatesPair> {
        tracing::debug!("location provider: resolving coordinates via GeoClue");
        let system = zbus::Connection::system().await?;
        let manager = GeoClueManagerProxy::new(&system).await?;
        let client_path = manager.create_client().await?;

        let result = async {
            let client = GeoClueClientProxy::builder(&system)
                .path(client_path.clone())?
                .build()
                .await?;
            client.set_desktop_id(GEOCLUE_DESKTOP_ID).await?;
            client
                .set_requested_accuracy_level(GEOCLUE_ACCURACY_LEVEL_CITY)
                .await?;
            client.start().await?;
            let coordinates = wait_for_geoclue_coordinates(&system, &client).await;
            let _ = client.stop().await;
            coordinates
        }
        .await;

        let _ = manager.delete_client(client_path).await;
        result
    }
}

async fn wait_for_geoclue_coordinates(
    system: &zbus::Connection,
    client: &GeoClueClientProxy<'_>,
) -> anyhow::Result<CoordinatesPair> {
    for _ in 0..GEOCLUE_LOCATION_POLL_ATTEMPTS {
        let path = client.location().await?;
        if let Some(coords) = coordinates_from_path(system, &path).await? {
            return Ok(coords);
        }
        tokio::time::sleep(std::time::Duration::from_millis(
            GEOCLUE_LOCATION_POLL_DELAY_MS,
        ))
        .await;
    }

    anyhow::bail!("GeoClue did not provide coordinates")
}

async fn coordinates_from_path(
    system: &zbus::Connection,
    path: &OwnedObjectPath,
) -> anyhow::Result<Option<CoordinatesPair>> {
    if path.as_str() == "/" {
        return Ok(None);
    }

    let location = GeoClueLocationProxy::builder(system)
        .path(path.clone())?
        .build()
        .await?;
    let latitude = location.latitude().await?;
    let longitude = location.longitude().await?;
    Ok(Some(validate_coordinates(latitude, longitude)?))
}

pub(crate) fn validate_coordinates(
    latitude: f64,
    longitude: f64,
) -> anyhow::Result<CoordinatesPair> {
    if !(-90.0..=90.0).contains(&latitude) {
        anyhow::bail!("latitude {latitude} is out of range");
    }
    if !(-180.0..=180.0).contains(&longitude) {
        anyhow::bail!("longitude {longitude} is out of range");
    }

    Ok(CoordinatesPair {
        latitude,
        longitude,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;

    use super::{CoordinatesPair, LocationProvider, LocationSource};

    struct MockSource {
        calls: Arc<AtomicUsize>,
        coordinates: CoordinatesPair,
    }

    #[async_trait]
    impl LocationSource for MockSource {
        async fn resolve_coordinates(&self) -> anyhow::Result<CoordinatesPair> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.coordinates)
        }
    }

    #[tokio::test]
    async fn geoclue_coordinates_are_used_when_config_is_missing() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = LocationProvider::with_source(Arc::new(MockSource {
            calls: calls.clone(),
            coordinates: CoordinatesPair {
                latitude: 40.7128,
                longitude: -74.006,
            },
        }));

        let resolved = provider
            .resolve_coordinates()
            .await
            .expect("geoclue coordinates");

        assert_eq!(
            resolved,
            CoordinatesPair {
                latitude: 40.7128,
                longitude: -74.006,
            }
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
