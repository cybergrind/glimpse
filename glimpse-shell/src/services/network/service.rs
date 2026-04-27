#![allow(dead_code)]

use std::{future, time::Duration};

use anyhow::{Context, anyhow};
use tokio::{
    sync::{mpsc, watch},
    time::sleep,
};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

use super::{
    Command, NetworkEvent, NetworkManagerClient, NetworkServiceHealth, State,
    protocol::active_action_for,
};

const COMMAND_QUEUE_SIZE: usize = 16;
const EVENT_QUEUE_SIZE: usize = 32;
const RETRY_DELAY: Duration = Duration::from_secs(2);

pub type NetworkHandle = ServiceHandle<State, Command>;

pub struct NetworkService {
    client: NetworkManagerClient,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    scan_interval: Option<Duration>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

impl NetworkService {
    pub fn new(conn: zbus::Connection) -> (Self, NetworkHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                client: NetworkManagerClient::new(conn),
                state_tx,
                command_rx,
                scan_interval: None,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        let mut reconnect_attempt = 0;

        loop {
            let outcome = match self.run_inner(cancel.clone()).await {
                Ok(outcome) => {
                    reconnect_attempt = 0;
                    outcome
                }
                Err(error) => {
                    reconnect_attempt += 1;
                    tracing::warn!(error = %error, "network service failed");
                    self.update_state(|state| {
                        state.health = NetworkServiceHealth::Reconnecting {
                            attempt: reconnect_attempt,
                        };
                    });
                    RunOutcome::RetryAfterDelay
                }
            };

            match outcome {
                RunOutcome::Cancelled => break,
                RunOutcome::RetryAfterDelay => {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sleep(RETRY_DELAY) => {}
                    }
                }
            }
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<RunOutcome> {
        tracing::debug!("network service started");
        self.refresh_snapshot()
            .await
            .context("failed to load initial network snapshot")?;

        let (event_tx, mut event_rx) = mpsc::channel(EVENT_QUEUE_SIZE);
        let listener_cancel = CancellationToken::new();
        let listener =
            spawn_network_listener(self.client.clone(), event_tx, listener_cancel.clone());

        let outcome = loop {
            tokio::select! {
                _ = cancel.cancelled() => break Ok(RunOutcome::Cancelled),
                event = event_rx.recv() => match event {
                    Some(NetworkEvent::Changed { reason }) => {
                        tracing::debug!(reason = %reason, "network: refreshing service state");
                        if let Err(error) = self.refresh_snapshot().await {
                            tracing::warn!(error = %error, "network: refresh failed after change event");
                            self.set_degraded("Network data is stale");
                        }
                    }
                    None => break Err(anyhow!("network event listener stopped")),
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        if self.execute_command(command).await {
                            if let Err(error) = self.refresh_snapshot().await {
                                tracing::warn!(error = %error, "network: refresh failed after command");
                                self.set_degraded("Network data is stale");
                            }
                        }
                    }
                    Some(ServiceCommand::Control(control)) => match control {
                        Control::Start(_) | Control::Reconfigure(_) => {}
                        Control::Shutdown => break Ok(RunOutcome::Cancelled),
                    },
                    None => break Ok(RunOutcome::Cancelled),
                },
                _ = async {
                    match self.scan_interval {
                        Some(interval) => sleep(interval).await,
                        None => future::pending::<()>().await,
                    }
                }, if self.scan_interval.is_some() => {
                    if let Err(error) = self.client.request_scan().await {
                        tracing::debug!(error = %error, "network: periodic scan request failed");
                    }
                    if let Err(error) = self.refresh_snapshot().await {
                        tracing::warn!(error = %error, "network: refresh failed after periodic scan");
                        self.set_degraded("Network data is stale");
                    }
                },
            }
        };

        listener_cancel.cancel();
        let _ = listener.await;

        outcome
    }

    async fn refresh_snapshot(&self) -> anyhow::Result<()> {
        let snapshot = self.client.scan().await?;
        self.update_state(|state| {
            state.health = health_after_successful_refresh(&state.health);
            state.snapshot = snapshot;
        });
        Ok(())
    }

    async fn execute_command(&mut self, command: Command) -> bool {
        let action = active_action_for(&command);
        if action.is_some() && self.state_tx.borrow().active_action.is_some() {
            tracing::warn!("network: command ignored while another action is active");
            return false;
        }

        if let Some(action) = action.clone() {
            self.update_state(|state| {
                state.active_action = Some(action);
            });
        }

        let result = self.execute_client_command(command).await;

        if action.is_some() {
            self.update_state(|state| {
                state.active_action = None;
            });
        }

        match result {
            Ok(refresh) => refresh,
            Err(error) => {
                tracing::warn!(error = %error, "network command failed");
                true
            }
        }
    }

    async fn execute_client_command(&mut self, command: Command) -> anyhow::Result<bool> {
        match command {
            Command::SetWifiEnabled(enabled) => {
                self.client.set_wifi_enabled(enabled).await?;
                Ok(true)
            }
            Command::StartScanning { interval_secs } => {
                self.client.request_scan().await?;
                let interval = scan_interval_duration(interval_secs);
                self.scan_interval = Some(interval);
                self.update_state(|state| {
                    state.scanning = true;
                });
                Ok(true)
            }
            Command::StopScanning => {
                self.scan_interval = None;
                self.update_state(|state| {
                    state.scanning = false;
                });
                Ok(false)
            }
            Command::RequestScan => {
                self.client.request_scan().await?;
                Ok(true)
            }
            Command::ConnectWifi { ssid, path } => {
                self.client.connect_access_point(&ssid, &path).await?;
                Ok(true)
            }
            Command::ConnectSaved { uuid } => {
                self.client.connect_uuid(&uuid).await?;
                Ok(true)
            }
            Command::Disconnect { uuid } => {
                self.client.disconnect(&uuid).await?;
                Ok(true)
            }
            Command::Forget { uuid } => {
                self.client.forget(&uuid).await?;
                Ok(true)
            }
        }
    }

    fn set_degraded(&self, message: &str) {
        self.update_state(|state| {
            state.health = NetworkServiceHealth::Degraded {
                message: message.into(),
            };
        });
    }

    fn update_state(&self, update: impl FnOnce(&mut State)) {
        let mut next = self.state_tx.borrow().clone();
        update(&mut next);
        if should_emit_state(&self.state_tx.borrow(), &next) {
            self.change_state(next);
        }
    }

    fn change_state(&self, state: State) {
        if self.state_tx.send(state).is_err() {
            tracing::debug!("network: state receiver dropped");
        }
    }
}

fn spawn_network_listener(
    client: NetworkManagerClient,
    events: mpsc::Sender<NetworkEvent>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move { client.listen(events, cancel).await })
}

fn should_emit_state(current: &State, next: &State) -> bool {
    current != next
}

fn scan_interval_duration(interval_secs: u64) -> Duration {
    Duration::from_secs(interval_secs.max(1))
}

fn health_after_successful_refresh(_current: &NetworkServiceHealth) -> NetworkServiceHealth {
    NetworkServiceHealth::Ready
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::network::NetworkActiveAction;

    #[test]
    fn active_action_tracks_long_running_device_commands() {
        assert_eq!(
            active_action_for(&Command::ConnectSaved { uuid: "id".into() }),
            Some(NetworkActiveAction::ConnectSaved { uuid: "id".into() })
        );
        assert_eq!(
            active_action_for(&Command::Disconnect { uuid: "id".into() }),
            Some(NetworkActiveAction::Disconnect { uuid: "id".into() })
        );
    }

    #[test]
    fn should_emit_state_only_for_real_changes() {
        let current = State::default();
        assert!(!should_emit_state(&current, &current));

        let mut next = current.clone();
        next.scanning = true;
        assert!(should_emit_state(&current, &next));
    }

    #[test]
    fn scan_interval_has_one_second_floor() {
        assert_eq!(scan_interval_duration(0), Duration::from_secs(1));
        assert_eq!(scan_interval_duration(10), Duration::from_secs(10));
    }

    #[test]
    fn successful_refresh_clears_transient_degraded_state() {
        assert_eq!(
            health_after_successful_refresh(&NetworkServiceHealth::Degraded {
                message: "Network data is stale".into(),
            }),
            NetworkServiceHealth::Ready
        );
    }
}
