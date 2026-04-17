use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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
        cancel: CancellationToken,
    ) -> Result<(), LocationError>;
}

pub struct LocationProvider {
    source: Box<dyn LocationSource>,
}

impl LocationProvider {
    pub fn new(source: Box<dyn LocationSource>) -> Self {
        Self { source }
    }

    pub async fn run(
        self,
        updates: mpsc::Sender<LocationEvent>,
        cancel: CancellationToken,
    ) -> Result<(), LocationError> {
        self.source.open(updates, cancel).await
    }
}
