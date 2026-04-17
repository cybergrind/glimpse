use async_trait::async_trait;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::services::location::provider::{
    Coordinates, LocationError, LocationEvent, LocationSource,
};

pub struct AresaSource {}

impl AresaSource {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl LocationSource for AresaSource {
    async fn open(
        self: Box<Self>,
        updates: mpsc::Sender<LocationEvent>,
        _cancel: CancellationToken,
    ) -> Result<(), LocationError> {
        let _ = updates;
        Ok(())
    }
}

pub struct StaticSource {
    coordinates: Coordinates,
}

impl StaticSource {
    pub fn new(coordinates: Coordinates) -> Self {
        Self { coordinates }
    }
}

#[async_trait]
impl LocationSource for StaticSource {
    async fn open(
        self: Box<Self>,
        updates: mpsc::Sender<LocationEvent>,
        _cancel: CancellationToken,
    ) -> Result<(), LocationError> {
        tracing::info!("static location provider emits, {:?}", self.coordinates);
        updates
            .send(LocationEvent::Update(self.coordinates))
            .await
            .map_err(|_| LocationError::Unavailable)
    }
}

pub struct GeoClueSource {}

impl GeoClueSource {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl LocationSource for GeoClueSource {
    async fn open(
        self: Box<Self>,
        updates: mpsc::Sender<LocationEvent>,
        _cancel: CancellationToken,
    ) -> Result<(), LocationError> {
        let _ = updates;
        Ok(())
    }
}
