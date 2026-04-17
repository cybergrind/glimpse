use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::services::location::service::LocationCommand;

#[derive(Debug, Clone, Deserialize, Copy, PartialEq)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

impl Coordinates {
    pub fn zero() -> Self {
        Self {
            latitude: 0.0,
            longitude: 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum LocationError {
    Unavailable,
    Other(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LocationEvent {
    Searching,
    Update(Coordinates),
    Unavailable,
}

#[async_trait]
pub trait LocationSource: Send + 'static {
    async fn open(
        self: Box<Self>,
        updates: mpsc::Sender<LocationEvent>,
        commands: mpsc::Receiver<LocationCommand>,
        cancel: CancellationToken,
    ) -> Result<(), LocationError>;
}

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
        mut commands: mpsc::Receiver<LocationCommand>,
        cancel: CancellationToken,
    ) -> Result<(), LocationError> {
        let _ = updates;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(LocationCommand::Refresh) = commands.recv() => {
                    tracing::debug!("aresa location source refresh requested");
                }
            }
        }

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
        mut commands: mpsc::Receiver<LocationCommand>,
        cancel: CancellationToken,
    ) -> Result<(), LocationError> {
        tracing::info!("static location source emits, {:?}", self.coordinates);
        updates
            .send(LocationEvent::Update(self.coordinates))
            .await
            .map_err(|_| LocationError::Unavailable)?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(LocationCommand::Refresh) = commands.recv() => {
                    tracing::debug!("static location source refresh requested");
                    updates
                        .send(LocationEvent::Update(self.coordinates))
                        .await
                        .map_err(|_| LocationError::Unavailable)?;
                }
            }
        }

        Ok(())
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
        mut commands: mpsc::Receiver<LocationCommand>,
        cancel: CancellationToken,
    ) -> Result<(), LocationError> {
        let _ = updates;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(LocationCommand::Refresh) = commands.recv() => {
                    tracing::debug!("geoclue location source refresh requested");
                }
            }
        }

        Ok(())
    }
}
