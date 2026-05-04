use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::LocationConfig;
use crate::services::{
    framework::{Control, ServiceCommand, ServiceHandle},
    geoclue,
};

#[derive(Debug, Clone)]
pub enum LocationError {
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone)]
pub enum State {
    Unknown,
    Ready(Coordinates),
    Refreshing,
    Degraded(LocationError),
}

#[derive(Debug, Clone)]
pub enum Command {
    Refresh,
}

pub type LocationHandle = ServiceHandle<State, Command>;

pub struct LocationService {
    geoclue: geoclue::GeoClueHandle,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

struct ActiveProvider {
    config: LocationConfig,
    task: JoinHandle<()>,
    cancel: CancellationToken,
    command_tx: mpsc::Sender<ProviderCommand>,
    message_rx: mpsc::Receiver<ProviderMessage>,
}

impl ActiveProvider {
    async fn cancel(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }

    async fn send(&self, command: ProviderCommand) {
        if let Err(e) = self.command_tx.send(command).await {
            tracing::error!("failed to send message to location provider: {:?}", e);
        }
    }

    fn spawn(config: LocationConfig, geoclue: geoclue::GeoClueHandle) -> Self {
        let (command_tx, command_rx) = mpsc::channel(1);
        let (message_tx, message_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = match config {
            LocationConfig::Static {
                latitude,
                longitude,
            } => tokio::spawn(async move {
                static_provider(latitude, longitude, command_rx, message_tx, task_cancel).await;
            }),
            LocationConfig::GeoClue => tokio::spawn(async move {
                geoclue_provider(geoclue, command_rx, message_tx, task_cancel).await;
            }),
            LocationConfig::IPAPI => tokio::spawn(async move {
                task_cancel.cancelled().await;
            }),
        };
        Self {
            cancel,
            config,
            task,
            message_rx,
            command_tx,
        }
    }
}

enum Lifecycle {
    Idle,
    Running(ActiveProvider),
}

impl LocationService {
    pub fn new(geoclue: geoclue::GeoClueHandle) -> (Self, LocationHandle) {
        let (state_tx, state_rx) = watch::channel(State::Unknown);
        let (command_tx, command_rx) = mpsc::channel(4);

        (
            Self {
                geoclue,
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    fn change_state(&self, state: State) {
        if let Err(err) = self.state_tx.send(state) {
            tracing::error!("failed to send new state: {:?}", err);
        }
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        tracing::debug!("location service started");
        let mut lifecycle = Lifecycle::Idle;

        loop {
            lifecycle = tokio::select! {
                _ = cancel.cancelled() => {
                    shutdown_task(lifecycle).await;
                    break;
                },
                provider_message = recv_from_provider(&mut lifecycle) => match provider_message {
                    Some(provider_message) => match provider_message {
                        ProviderMessage::Value(coordinates) => {
                            tracing::debug!("location received: {:?}", coordinates);
                            self.change_state(State::Ready(coordinates));
                            lifecycle
                        },
                        ProviderMessage::Unavailable(err) => {
                            tracing::debug!("location service degraded: {:?}", err);
                            self.change_state(State::Degraded(err));
                            lifecycle
                        },
                    },
                    None => lifecycle
                },
                command_message = self.command_rx.recv() => match command_message{
                    Some(command_message) => match command_message{
                        ServiceCommand::Control(control_command) => match control_command {
                            Control::Start(config) => {
                                tracing::debug!("start location provider: {}", config.location);
                                shutdown_task(lifecycle).await;
                                Lifecycle::Running(ActiveProvider::spawn(config.location.clone(), self.geoclue.clone()))
                            },
                            Control::Reconfigure(ref config) => match lifecycle {
                                Lifecycle::Running(ref provider) =>  {
                                    let new_config = config.location.clone();
                                    if new_config != provider.config {
                                        tracing::debug!("reconfiguring location service: {}", new_config);
                                        shutdown_task(lifecycle).await;
                                         Lifecycle::Running(ActiveProvider::spawn(new_config, self.geoclue.clone()))
                                    } else {
                                        lifecycle
                                    }
                                },
                                _ => lifecycle,
                            },
                            Control::Shutdown => {
                                tracing::debug!("location service shutting down");
                                shutdown_task(lifecycle).await;
                                break;
                            },
                        },
                        ServiceCommand::Command(service_command) => match service_command {
                            Command::Refresh => match lifecycle {
                                Lifecycle::Running(ref provider) => {
                                    self.change_state(State::Refreshing);
                                    provider.send(ProviderCommand::Refresh).await;
                                    lifecycle
                                },
                                _ => {
                                    tracing::debug!("refresh message dropped because the service is not running");
                                    lifecycle
                                },
                            },
                        },
                    },
                    None => {
                        tracing::debug!("command channel closed");
                        break
                    }
                }
            };
        }

        tracing::debug!("location service quit");
    }
}

async fn shutdown_task(state: Lifecycle) {
    if let Lifecycle::Running(task) = state {
        task.cancel().await;
    }
}

enum ProviderCommand {
    Refresh,
}
enum ProviderMessage {
    Value(Coordinates),
    Unavailable(LocationError),
}

async fn static_provider(
    latitude: f64,
    longitude: f64,
    mut command_receiver: mpsc::Receiver<ProviderCommand>,
    value_sender: mpsc::Sender<ProviderMessage>,
    cancel: CancellationToken,
) {
    tracing::debug!("static location provider started");
    let _ = value_sender
        .send(ProviderMessage::Value(Coordinates {
            latitude,
            longitude,
        }))
        .await;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            command = command_receiver.recv() => match command {
                Some(command) => match command {
                    ProviderCommand::Refresh => {
                        if let Err(e) = value_sender.send(ProviderMessage::Value(Coordinates { latitude, longitude })).await {
                            tracing::error!("failed to send result from location provider: {:?}", e);
                        }
                    }
                },
                None => break,
            }
        }
    }
}

async fn geoclue_provider(
    geoclue: geoclue::GeoClueHandle,
    mut command_rx: mpsc::Receiver<ProviderCommand>,
    message_tx: mpsc::Sender<ProviderMessage>,
    cancel: CancellationToken,
) {
    let mut state_rx = geoclue.subscribe();
    let state = { state_rx.borrow().clone() };
    publish_geoclue_location(state, &message_tx).await;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            changed = state_rx.changed() => {
                if changed.is_err() {
                    let _ = message_tx.send(ProviderMessage::Unavailable(LocationError::Unavailable)).await;
                    break;
                }
                let state = { state_rx.borrow().clone() };
                publish_geoclue_location(state, &message_tx).await;
            }
            command = command_rx.recv() => match command {
                Some(ProviderCommand::Refresh) => {
                    let state = { state_rx.borrow().clone() };
                    publish_geoclue_location(state, &message_tx).await;
                }
                None => break,
            }
        }
    }
}

async fn publish_geoclue_location(
    state: geoclue::State,
    message_tx: &mpsc::Sender<ProviderMessage>,
) {
    if let Some(coordinates) = &state.coordinates {
        let _ = message_tx
            .send(ProviderMessage::Value(Coordinates {
                latitude: coordinates.latitude,
                longitude: coordinates.longitude,
            }))
            .await;
    } else if !state.available || state.error.is_some() {
        let _ = message_tx
            .send(ProviderMessage::Unavailable(LocationError::Unavailable))
            .await;
    }
}

async fn recv_from_provider(state: &mut Lifecycle) -> Option<ProviderMessage> {
    match state {
        Lifecycle::Running(provider) => provider.message_rx.recv().await,
        _ => std::future::pending().await,
    }
}
