use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::services::control::ControlEvent;
use crate::services::framework::{RunningTask, ServiceEvent};

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
    state_tx: watch::Sender<State>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocationSourceType {
    IPAPI,
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

struct ActiveSource {
    config: LocationConfig,
    running_task: RunningTask<LocationCommand>,
    output_rx: mpsc::Receiver<LocationEvent>,
}

impl ActiveSource {
    async fn stop(self) {
        self.running_task.cancel().await
    }
}

enum ServiceLifecycle {
    Idle,
    Running(ActiveSource),
}

enum Input {
    Cancelled,
    Command(ServiceEvent<LocationCommand>),
    Output(LocationEvent),
    SourceClosed,
}

async fn recv_source_event(lifecycle: &mut ServiceLifecycle) -> Option<LocationEvent> {
    match lifecycle {
        ServiceLifecycle::Running(active) => active.output_rx.recv().await,
        _ => std::future::pending().await,
    }
}

impl LocationService {
    pub fn new() -> (Self, LocationServiceHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        (Self { state_tx }, LocationServiceHandle { state_rx })
    }

    pub async fn run(
        self,
        mut commands: mpsc::Receiver<ServiceEvent<LocationCommand>>,
        cancel: CancellationToken,
    ) {
        let state_tx = self.state_tx;
        let mut state = state_tx.borrow().clone();
        let mut lifecycle = ServiceLifecycle::Idle;

        loop {
            let input = tokio::select! {
              _ = cancel.cancelled() => Input::Cancelled,
              command = commands.recv() => match command {
                  Some(command) => Input::Command(command),
                  None => Input::Cancelled,
              },
              src = recv_source_event(&mut lifecycle) => match src  {
                  Some(event) => Input::Output(event),
                  None => Input::SourceClosed,
              }
            };

            lifecycle = match input {
                Input::Cancelled => match lifecycle {
                    ServiceLifecycle::Running(active) => {
                        active.stop().await;
                        break;
                    }
                    _ => break,
                },
                Input::Output(message) => match message {
                    LocationEvent::Searching => {
                        state.status = LocationStatus::Searching;
                        let _ = state_tx.send(state.clone());
                        lifecycle
                    }
                    LocationEvent::Unavailable => {
                        state.status = LocationStatus::Degraded(LocationError::Other(
                            "Location unavailable".into(),
                        ));
                        let _ = state_tx.send(state.clone());
                        lifecycle
                    }
                    LocationEvent::Update(coordinates) => {
                        state.status = LocationStatus::Ready;
                        state.coordinates = Some(coordinates);
                        let _ = state_tx.send(state.clone());
                        lifecycle
                    }
                },
                Input::SourceClosed => match lifecycle {
                    ServiceLifecycle::Running(active) => {
                        tracing::warn!("location source channel closed");
                        active.stop().await;
                        state.status = LocationStatus::Degraded(LocationError::Other(
                            "location source stopped".into(),
                        ));
                        let _ = state_tx.send(state.clone());
                        ServiceLifecycle::Idle
                    }
                    _ => lifecycle,
                },
                Input::Command(ServiceEvent::Command(service_command)) => match service_command {
                    LocationCommand::Refresh => match lifecycle {
                        ServiceLifecycle::Running(active) => {
                            let _ = active.running_task.send(service_command).await;
                            ServiceLifecycle::Running(active)
                        }
                        _ => lifecycle,
                    },
                },
                Input::Command(ServiceEvent::Control(control_command)) => match control_command {
                    ControlEvent::Shutdown => match lifecycle {
                        ServiceLifecycle::Running(active) => {
                            active.stop().await;
                            break;
                        }
                        _ => break,
                    },
                    ControlEvent::Configure(config) => match lifecycle {
                        ServiceLifecycle::Idle => {
                            let new_location_config = LocationConfig::from_app_config(&config);
                            let (running_task, output_rx) =
                                activate_source(new_location_config.clone());
                            if let Err(e) = running_task.send(LocationCommand::Refresh).await {
                                tracing::warn!("failed to trigger initial location refresh: {e:?}");
                            }
                            ServiceLifecycle::Running(ActiveSource {
                                running_task,
                                output_rx,
                                config: new_location_config,
                            })
                        }
                        ServiceLifecycle::Running(active) => {
                            let new_config = LocationConfig::from_app_config(&config);
                            if new_config == active.config {
                                tracing::debug!("location config did not change");
                                ServiceLifecycle::Running(active)
                            } else {
                                tracing::info!("configuring location service");
                                active.stop().await;

                                let (running_task, output_rx) = activate_source(new_config.clone());
                                if let Err(e) = running_task.send(LocationCommand::Refresh).await {
                                    tracing::warn!(
                                        "failed to trigger location refresh after reconfigure: {e:?}"
                                    );
                                }
                                ServiceLifecycle::Running(ActiveSource {
                                    running_task,
                                    output_rx,
                                    config: new_config,
                                })
                            }
                        }
                    },
                },
            };
        }
    }
}

fn activate_source(
    config: LocationConfig,
) -> (RunningTask<LocationCommand>, mpsc::Receiver<LocationEvent>) {
    let cancel = CancellationToken::new();
    let (command_tx, command_rx) = mpsc::channel(8);
    let (event_tx, event_rx) = mpsc::channel(16);

    let task = match config.source {
        LocationSourceType::Static => tokio::spawn(spawn_static_source(
            config.clone(),
            command_rx,
            event_tx,
            cancel.clone(),
        )),
        LocationSourceType::IPAPI => tokio::spawn(spawn_static_source(
            config.clone(),
            command_rx,
            event_tx,
            cancel.clone(),
        )),
        LocationSourceType::GeoClue => tokio::spawn(spawn_static_source(
            config.clone(),
            command_rx,
            event_tx,
            cancel.clone(),
        )),
    };

    (
        RunningTask {
            task,
            cancel,
            command_tx,
        },
        event_rx,
    )
}

async fn spawn_static_source(
    config: LocationConfig,
    mut command_rx: mpsc::Receiver<LocationCommand>,
    output_tx: mpsc::Sender<LocationEvent>,
    cancel: CancellationToken,
) {
    tracing::debug!("static source initial data");
    if let (Some(latitude), Some(longitude)) = (config.latitude, config.longitude) {
        if let Err(e) = output_tx.try_send(LocationEvent::Update(Coordinates {
            latitude,
            longitude,
        })) {
            tracing::debug!("static source: failed to emit initial update: {:?}", e);
        }
    }

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                break;
            },
            command = command_rx.recv() => {
                match command {
                    Some(LocationCommand::Refresh) => {
                        tracing::debug!("static source refresh requested");
                        let (Some(latitude), Some(longitude)) = (config.latitude, config.longitude) else {
                            tracing::warn!("value of latitude or longitude cannot be null: lat={:?}, lng={:?}", config.latitude, config.longitude);
                            continue;
                        };

                        if let Err(e) = output_tx.try_send(LocationEvent::Update(Coordinates { latitude, longitude })) {
                            tracing::debug!("static source: failed to emit refresh update: {:?}", e);
                        }
                    },
                    None => {
                        tracing::debug!("static source channel closed");
                        break;
                    }
                };
            }
        }
    }
}

// http://ip-api.com/json/
// {
//   "status": "success",
//   "country": "Poland",
//   "countryCode": "PL",
//   "region": "14",
//   "regionName": "Mazovia",
//   "city": "Warsaw",
//   "zip": "02-230",
//   "lat": 52.183,
//   "lon": 20.9273,
//   "timezone": "Europe/Warsaw",
//   "isp": "UPC.pl",
//   "org": "UPC Polska Sp. z o.o.",
//   "as": "AS9141 P4 Sp. z o.o.",
//   "query": "89.67.177.131"
// }
