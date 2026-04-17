use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::services::{
    control::ControlEvent,
    location::{
        provider::{Coordinates, LocationError, LocationEvent, LocationProvider, LocationSource},
        sources::{AresaSource, GeoClueSource, StaticSource},
    },
};

#[derive(Debug, Clone)]
pub enum LocationStatus {
    Ready,
    Searching,
    Degraded(LocationError),
}

#[derive(Debug, Clone)]
pub struct State {
    coordinates: Option<Coordinates>,
    status: LocationStatus,
}

impl Default for State {
    fn default() -> Self {
        Self {
            coordinates: None,
            status: LocationStatus::Searching,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocationConfig {
    pub source: LocationSourceType,
    pub longitude: Option<f64>,
    pub latitude: Option<f64>,
}

pub struct LocationService {
    config: LocationConfig,
    state_tx: watch::Sender<State>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocationSourceType {
    Aresa,
    #[default]
    GeoClue,
    Static,
}

#[derive(Debug, Clone)]
pub struct LocationServiceHandle {
    state_rx: watch::Receiver<State>,
}

impl LocationServiceHandle {
    pub fn snapshot(&self) -> State {
        self.state_rx.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<State> {
        self.state_rx.clone()
    }
}

struct RunningProvider {
    task: JoinHandle<()>,
    cancel: CancellationToken,
}

impl RunningProvider {
    fn spawn(config: &LocationConfig) -> (Self, mpsc::Receiver<LocationEvent>) {
        let provider = LocationProvider::new(make_provider_source(config));
        let (provider_tx, provider_rx) = mpsc::channel::<LocationEvent>(16);

        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();

        let task = tokio::spawn(async move {
            if let Err(err) = provider.run(provider_tx, task_cancel).await {
                tracing::error!("location provider crashed: {:?}", err);
            }
        });

        (RunningProvider { task, cancel }, provider_rx)
    }

    async fn cancel(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }
}

impl LocationService {
    pub fn new(config: LocationConfig) -> (Self, LocationServiceHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        (
            Self { state_tx, config },
            LocationServiceHandle { state_rx },
        )
    }

    pub async fn run(
        self,
        cancel: CancellationToken,
        mut control: mpsc::Receiver<ControlEvent>,
    ) -> Result<(), LocationError> {
        let LocationService {
            mut config,
            state_tx,
        } = self;
        let mut state = state_tx.borrow().clone();
        let (mut running_provider, mut provider_rx) = RunningProvider::spawn(&config);

        state.status = LocationStatus::Searching;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    running_provider.cancel().await;
                    break;
                },
                Some(control) = control.recv() => {
                    match control {
                        ControlEvent::Reconfigure(new_config) => {
                            let new_location_config = LocationConfig{
                                source: new_config.location.source.clone(),
                                latitude: new_config.location.latitude,
                                longitude: new_config.location.longitude,
                            };
                            if config == new_location_config {
                                tracing::debug!("location config did not change");
                                continue;
                            }

                            tracing::info!("reconfiguring location service because of configuration change");
                            running_provider.cancel().await;
                            (running_provider, provider_rx) = RunningProvider::spawn(&new_location_config);
                            config = new_location_config;
                            state.status = LocationStatus::Searching;
                            let _ = state_tx.send(state.clone());
                        },
                    }
                },
                maybe_event = provider_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            tracing::info!("received provider event: {:?}", maybe_event);
                            match event {
                                LocationEvent::Searching => {
                                    state.status = LocationStatus::Searching;
                                }
                                LocationEvent::Update(coordinates) => {
                                    state.status = LocationStatus::Ready;
                                    state.coordinates = Some(coordinates);
                                }
                                LocationEvent::Unavailable => state.status = LocationStatus::Degraded(LocationError::Unavailable)
                            }
                        }
                        None => {
                            tracing::warn!("location provider stopped");
                            state.status = LocationStatus::Degraded(LocationError::Other("provider stopped".into()));
                            let _ = state_tx.send(state.clone());
                            running_provider.cancel().await;
                            break;
                        }
                    }

                    let _ = state_tx.send(state.clone());
                },
            }
        }

        Ok(())
    }
}

fn make_provider_source(config: &LocationConfig) -> Box<dyn LocationSource> {
    match config.source {
        LocationSourceType::GeoClue => Box::new(GeoClueSource::new()),
        LocationSourceType::Aresa => Box::new(AresaSource::new()),
        LocationSourceType::Static => {
            if let (Some(lat), Some(lon)) = (config.latitude, config.longitude) {
                return Box::new(StaticSource::new(Coordinates {
                    latitude: lat,
                    longitude: lon,
                }));
            }

            tracing::warn!("static location: either latitude or longitude is not set");
            Box::new(StaticSource::new(Coordinates::zero()))
        }
    }
}
