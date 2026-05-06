#![allow(dead_code)]

use std::time::Duration;

use anyhow::{Context, anyhow};
use tokio::{
    sync::{mpsc, watch},
    time::sleep,
};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

use super::{
    BluetoothActiveAction, BluetoothServiceHealth, BluezClient, BluezEvent, Command, State,
};

const COMMAND_QUEUE_SIZE: usize = 16;
const EVENT_QUEUE_SIZE: usize = 32;
const RETRY_DELAY: Duration = Duration::from_secs(2);

pub type BluetoothHandle = ServiceHandle<State, Command>;

pub struct BluetoothService {
    client: BluezClient,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

impl BluetoothService {
    pub fn new(conn: zbus::Connection) -> (Self, BluetoothHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                client: BluezClient::new(conn),
                state_tx,
                command_rx,
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
                    tracing::warn!(error = %error, "bluetooth service failed");
                    self.update_state(|state| {
                        state.health = BluetoothServiceHealth::Reconnecting {
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
        tracing::debug!("bluetooth service started");
        self.refresh_snapshot()
            .await
            .context("failed to load initial bluetooth snapshot")?;

        let (event_tx, mut event_rx) = mpsc::channel(EVENT_QUEUE_SIZE);
        let listener_cancel = CancellationToken::new();
        let listener = spawn_bluez_listener(self.client.clone(), event_tx, listener_cancel.clone());

        let outcome = loop {
            tokio::select! {
                _ = cancel.cancelled() => break Ok(RunOutcome::Cancelled),
                event = event_rx.recv() => match event {
                    Some(BluezEvent::Changed { reason }) => {
                        tracing::debug!(reason = %reason, "bluetooth: refreshing service state");
                        if let Err(error) = self.refresh_snapshot().await {
                            tracing::warn!(error = %error, "bluetooth: refresh failed after change event");
                            self.set_degraded("Bluetooth data is stale");
                        }
                    }
                    None => break Err(anyhow!("bluetooth event listener stopped")),
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        if self.execute_command(command).await {
                            if let Err(error) = self.refresh_snapshot().await {
                                tracing::warn!(error = %error, "bluetooth: refresh failed after command");
                                self.set_degraded("Bluetooth data is stale");
                            }
                        }
                    }
                    Some(ServiceCommand::Control(control)) => match control {
                        Control::Start(_) | Control::Reconfigure(_) => {}
                        Control::Shutdown => break Ok(RunOutcome::Cancelled),
                    },
                    None => break Ok(RunOutcome::Cancelled),
                }
            }
        };

        listener_cancel.cancel();
        let _ = listener.await;

        outcome
    }

    async fn refresh_snapshot(&self) -> anyhow::Result<()> {
        let snapshot = self.client.scan().await?;
        self.update_state(|state| {
            if !matches!(state.health, BluetoothServiceHealth::Degraded { .. }) {
                state.health = BluetoothServiceHealth::Ready;
            }
            state.snapshot = snapshot;
        });
        Ok(())
    }

    async fn execute_command(&self, command: Command) -> bool {
        let action = active_action_for(&command);
        if action.is_some() && self.state_tx.borrow().active_action.is_some() {
            tracing::warn!("bluetooth: command ignored while another action is active");
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
                tracing::warn!(error = %error, "bluetooth command failed");
                true
            }
        }
    }

    async fn execute_client_command(&self, command: Command) -> anyhow::Result<bool> {
        match command {
            Command::SetPowered(powered) => {
                self.client.set_powered(powered).await?;
                Ok(true)
            }
            Command::SetAdapterPowered {
                adapter_path,
                powered,
            } => {
                self.client
                    .set_adapter_powered(&adapter_path, powered)
                    .await?;
                Ok(true)
            }
            Command::SetAdapterDiscoverable {
                adapter_path,
                discoverable,
            } => {
                self.client
                    .set_adapter_discoverable(&adapter_path, discoverable)
                    .await?;
                Ok(true)
            }
            Command::StartDiscovery => {
                self.client.start_discovery().await?;
                Ok(true)
            }
            Command::StopDiscovery => {
                self.client.stop_discovery().await?;
                Ok(true)
            }
            Command::Connect { address } => {
                self.client.connect(&address).await?;
                Ok(true)
            }
            Command::Disconnect { address } => {
                self.client.disconnect(&address).await?;
                Ok(true)
            }
            Command::Pair { address } => {
                tracing::debug!(address = %address, "bluetooth: pair command started");
                self.client.pair(&address).await?;
                tracing::debug!(address = %address, "bluetooth: pair command finished");
                Ok(true)
            }
            Command::Trust { address, trusted } => {
                tracing::debug!(
                    address = %address,
                    trusted,
                    "bluetooth: trust command started"
                );
                self.client.trust(&address, trusted).await?;
                tracing::debug!(
                    address = %address,
                    trusted,
                    "bluetooth: trust command finished"
                );
                Ok(true)
            }
            Command::Forget { address } => {
                tracing::debug!(address = %address, "bluetooth: forget command started");
                self.client.forget(&address).await?;
                tracing::debug!(address = %address, "bluetooth: forget command finished");
                Ok(true)
            }
        }
    }

    fn set_degraded(&self, message: &str) {
        self.update_state(|state| {
            state.health = BluetoothServiceHealth::Degraded {
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
        if let Err(error) = self.state_tx.send(state) {
            tracing::error!("failed to send new bluetooth state: {:?}", error);
        }
    }
}

fn spawn_bluez_listener(
    client: BluezClient,
    events: mpsc::Sender<BluezEvent>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = client.listen(events, cancel).await {
            tracing::warn!(error = %error, "bluetooth listener failed");
        }
    })
}

fn active_action_for(command: &Command) -> Option<BluetoothActiveAction> {
    match command {
        Command::SetPowered(powered) => Some(BluetoothActiveAction::SetPowered(*powered)),
        Command::SetAdapterPowered {
            adapter_path,
            powered,
        } => Some(BluetoothActiveAction::SetAdapterPowered {
            adapter_path: adapter_path.clone(),
            powered: *powered,
        }),
        Command::SetAdapterDiscoverable {
            adapter_path,
            discoverable,
        } => Some(BluetoothActiveAction::SetAdapterDiscoverable {
            adapter_path: adapter_path.clone(),
            discoverable: *discoverable,
        }),
        Command::Connect { address } => Some(BluetoothActiveAction::Connect {
            address: address.clone(),
        }),
        Command::Disconnect { address } => Some(BluetoothActiveAction::Disconnect {
            address: address.clone(),
        }),
        Command::Pair { address } => Some(BluetoothActiveAction::Pair {
            address: address.clone(),
        }),
        Command::Trust { address, trusted } => Some(BluetoothActiveAction::Trust {
            address: address.clone(),
            trusted: *trusted,
        }),
        Command::Forget { address } => Some(BluetoothActiveAction::Forget {
            address: address.clone(),
        }),
        Command::StartDiscovery | Command::StopDiscovery => None,
    }
}

fn should_emit_state(previous: &State, next: &State) -> bool {
    previous != next
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::bluetooth::{
        BluetoothActiveAction, BluetoothServiceHealth, Command, State,
    };

    #[test]
    fn active_action_tracks_long_running_device_commands() {
        assert_eq!(
            active_action_for(&Command::Connect {
                address: "AA:BB".into()
            }),
            Some(BluetoothActiveAction::Connect {
                address: "AA:BB".into()
            })
        );
        assert_eq!(
            active_action_for(&Command::Trust {
                address: "AA:BB".into(),
                trusted: true,
            }),
            Some(BluetoothActiveAction::Trust {
                address: "AA:BB".into(),
                trusted: true,
            })
        );
    }

    #[test]
    fn discovery_commands_do_not_claim_active_action() {
        assert_eq!(active_action_for(&Command::StartDiscovery), None);
        assert_eq!(active_action_for(&Command::StopDiscovery), None);
    }

    #[test]
    fn should_emit_state_only_for_real_changes() {
        let previous = State::default();
        assert!(!should_emit_state(&previous, &previous));

        let mut next = previous.clone();
        next.health = BluetoothServiceHealth::Ready;
        assert!(should_emit_state(&previous, &next));
    }
}
