use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::services::framework::{RunningTask, ServiceEvent};
use crate::services::{
    control::ControlEvent,
    location::sources::{
        AresaSource, Coordinates, GeoClueSource, LocationError, LocationEvent, LocationSource,
        StaticSource,
    },
};

pub enum LocationCommand {
    Refresh,
}

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

impl LocationConfig {
    pub fn from_app_config(config: &Config) -> Self {
        Self {
            source: config.location.source.clone(),
            latitude: config.location.latitude,
            longitude: config.location.longitude,
        }
    }
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
        mut events: mpsc::Receiver<ServiceEvent<LocationCommand>>,
    ) -> Result<(), LocationError> {
        let LocationService {
            mut config,
            state_tx,
        } = self;
        let mut state = state_tx.borrow().clone();
        let (mut source, mut source_rx) = spawn_source(&config);

        state.status = LocationStatus::Searching;
        let _ = state_tx.send(state.clone());

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    source.cancel().await;
                    break;
                },
                Some(event) = events.recv() => {
                    match event {
                        ServiceEvent::Control(ControlEvent::Reconfigure(new_config)) => {
                            let new_location_config = LocationConfig::from_app_config(&new_config);
                            if config == new_location_config {
                                tracing::debug!("location config did not change");
                                continue;
                            }

                            tracing::info!("reconfiguring location service because of configuration change");
                            (source, source_rx) = restart_source(source, &new_location_config).await;

                            config = new_location_config;
                            state.status = LocationStatus::Searching;
                            let _ = state_tx.send(state.clone());
                        },
                        ServiceEvent::Control(ControlEvent::Shutdown) => {
                            source.cancel().await;
                            break;
                        }
                        ServiceEvent::Command(LocationCommand::Refresh) => {
                            let _ = source.send(LocationCommand::Refresh).await;
                        }
                    }
                },
                maybe_event = source_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            tracing::info!("received source event: {:?}", event);
                            match event {
                                LocationEvent::Searching => {
                                    state.status = LocationStatus::Searching;
                                }
                                LocationEvent::Update(coordinates) => {
                                    state.status = LocationStatus::Ready;
                                    state.coordinates = Some(coordinates);
                                }
                                LocationEvent::Unavailable => {
                                    state.status = LocationStatus::Degraded(LocationError::Unavailable);
                                }
                            }
                        }
                        None => {
                            tracing::warn!("location source stopped");
                            state.status = LocationStatus::Degraded(LocationError::Other("source stopped".into()));
                            let _ = state_tx.send(state.clone());
                            source.cancel().await;
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

async fn restart_source(
    source: RunningTask<LocationCommand>,
    config: &LocationConfig,
) -> (RunningTask<LocationCommand>, mpsc::Receiver<LocationEvent>) {
    source.cancel().await;
    spawn_source(config)
}

fn make_source(config: &LocationConfig) -> Box<dyn LocationSource> {
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

fn spawn_source(
    config: &LocationConfig,
) -> (RunningTask<LocationCommand>, mpsc::Receiver<LocationEvent>) {
    let source = make_source(config);

    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();

    let (command_tx, mut command_rx) = mpsc::channel(8);
    let (event_tx, event_rx) = mpsc::channel(16);

    let task = tokio::spawn(async move {
        if let Err(error) = source.open(event_tx.clone(), task_cancel.clone()).await {
            tracing::error!(error = ?error, "failed to start location source");
            let _ = event_tx.send(LocationEvent::Unavailable).await;
            return;
        }

        loop {
            tokio::select! {
                _ = task_cancel.cancelled() => {
                    break;
                }
                Some(command) = command_rx.recv() => {
                    match command {
                        LocationCommand::Refresh => {
                            tracing::debug!("location source refresh requested");
                        }
                    }
                }
            }
        }
    });

    (
        RunningTask {
            task,
            control_tx: command_tx,
            cancel,
        },
        event_rx,
    )
}
