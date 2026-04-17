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
        let LocationService { mut config, state_tx } = self;
        let mut state = state_tx.borrow().clone();
        let (mut source, mut source_rx) = activate_source(&config, &mut state, &state_tx);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    if let Some(source) = source.take() {
                        source.cancel().await;
                    }
                    break;
                },
                Some(event) = events.recv() => {
                    match event {
                        ServiceEvent::Control(ControlEvent::Reconfigure(new_config)) => {
                            let new_location_config = LocationConfig::from_app_config(&new_config);
                            if source.is_some() && config == new_location_config {
                                tracing::debug!("location config did not change");
                                continue;
                            }

                            tracing::info!("reconfiguring location service because of configuration change");
                            if let Some(current) = source.take() {
                                current.cancel().await;
                            }
                            config = new_location_config;
                            (source, source_rx) = activate_source(&config, &mut state, &state_tx);
                        },
                        ServiceEvent::Control(ControlEvent::Shutdown) => {
                            if let Some(source) = source.take() {
                                source.cancel().await;
                            }
                            break;
                        }
                        ServiceEvent::Command(LocationCommand::Refresh) => {
                            if let Some(source) = source.as_ref() {
                                let _ = source.send(LocationCommand::Refresh).await;
                            } else {
                                tracing::info!("refresh requested with no running source; restarting location source");
                                (source, source_rx) = activate_source(&config, &mut state, &state_tx);
                            }
                        }
                    }
                },
                maybe_event = async {
                    match &mut source_rx {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
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
                            let _ = state_tx.send(state.clone());
                        }
                        None => {
                            tracing::warn!("location source stopped");
                            if let Some(current) = source.take() {
                                current.cancel().await;
                            }
                            source_rx = None;
                            state.status = LocationStatus::Degraded(LocationError::Other("source stopped".into()));
                            let _ = state_tx.send(state.clone());
                        }
                    }
                },
            }
        }

        Ok(())
    }
}

fn activate_source(
    config: &LocationConfig,
    state: &mut State,
    state_tx: &watch::Sender<State>,
) -> (
    Option<RunningTask<LocationCommand>>,
    Option<mpsc::Receiver<LocationEvent>>,
) {
    match spawn_source(config) {
        Ok((source, receiver)) => {
            state.status = LocationStatus::Searching;
            let _ = state_tx.send(state.clone());
            (Some(source), Some(receiver))
        }
        Err(error) => {
            tracing::warn!(error = ?error, "failed to start location source");
            state.status = LocationStatus::Degraded(error);
            let _ = state_tx.send(state.clone());
            (None, None)
        }
    }
}

fn make_source(config: &LocationConfig) -> Result<Box<dyn LocationSource>, LocationError> {
    match config.source {
        LocationSourceType::GeoClue => Ok(Box::new(GeoClueSource::new())),
        LocationSourceType::Aresa => Ok(Box::new(AresaSource::new())),
        LocationSourceType::Static => {
            let (Some(latitude), Some(longitude)) = (config.latitude, config.longitude) else {
                return Err(LocationError::Other(
                    "static location requires both latitude and longitude".into(),
                ));
            };

            Ok(Box::new(StaticSource::new(Coordinates {
                latitude,
                longitude,
            })))
        }
    }
}

fn spawn_source(
    config: &LocationConfig,
) -> Result<(RunningTask<LocationCommand>, mpsc::Receiver<LocationEvent>), LocationError> {
    let source = make_source(config)?;

    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();

    let (command_tx, command_rx) = mpsc::channel(8);
    let (event_tx, event_rx) = mpsc::channel(16);

    let task = tokio::spawn(async move {
        if let Err(error) = source.open(event_tx.clone(), command_rx, task_cancel).await {
            tracing::error!(error = ?error, "location source exited with error");
            let _ = event_tx.send(LocationEvent::Unavailable).await;
        }
    });

    Ok((
        RunningTask {
            task,
            control_tx: command_tx,
            cancel,
        },
        event_rx,
    ))
}
