use std::future;

use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum LocationConfig {
    Static {
        latitude: f64,
        longitude: f64,
    },
    GeoClue,
    #[serde(rename = "ipapi")]
    IPAPI,
}

impl Default for LocationConfig {
    fn default() -> Self {
        Self::GeoClue
    }
}

impl std::fmt::Display for LocationConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static {
                latitude,
                longitude,
            } => write!(f, "static({latitude}, {longitude})"),
            Self::GeoClue => f.write_str("geoclue"),
            Self::IPAPI => f.write_str("ipapi"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum LocationError {}

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

pub struct LocationService {
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

    fn spawn(config: LocationConfig) -> Self {
        let (command_tx, command_rx) = mpsc::channel(1);
        let (message_tx, message_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = match config {
            LocationConfig::Static {
                latitude,
                longitude,
            } => tokio::spawn(async move {
                static_provider(
                    latitude,
                    longitude,
                    command_rx,
                    message_tx,
                    task_cancel.clone(),
                )
                .await;
            }),
            LocationConfig::GeoClue => tokio::spawn(async move {
                let () = future::pending().await;
            }),
            LocationConfig::IPAPI => tokio::spawn(async move {
                let () = future::pending().await;
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
    pub fn new() -> (Self, ServiceHandle<State, Command>) {
        let (state_tx, state_rx) = watch::channel(State::Unknown);
        let (command_tx, command_rx) = mpsc::channel(4);

        (
            Self {
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
                                Lifecycle::Running(ActiveProvider::spawn(config.location.clone()))
                            },
                            Control::Reconfigure(ref config) => match lifecycle {
                                Lifecycle::Running(ref provider) =>  {
                                    let new_config = config.location.clone();
                                    if new_config != provider.config {
                                        tracing::debug!("reconfiguring location service: {}", new_config);
                                        shutdown_task(lifecycle).await;
                                         Lifecycle::Running(ActiveProvider::spawn(new_config))
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
                None => {}
            }
        }
    }
}

async fn recv_from_provider(state: &mut Lifecycle) -> Option<ProviderMessage> {
    match state {
        Lifecycle::Running(provider) => provider.message_rx.recv().await,
        _ => std::future::pending().await,
    }
}
