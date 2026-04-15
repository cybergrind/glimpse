use std::{
    error::Error,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    bluetooth::provider::{BluetoothProvider, BluetoothProviderEvent},
    bluetooth::{
        agent::{BluetoothAgent, PromptRegistry},
        protocol::{
            BluetoothActiveAction, BluetoothServiceCommand, BluetoothServiceHealth,
            BluetoothServiceState,
        },
    },
};

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

#[derive(Clone)]
pub struct BluetoothServiceHandle {
    commands: mpsc::Sender<BluetoothServiceCommand>,
    state: watch::Receiver<BluetoothServiceState>,
}

impl BluetoothServiceHandle {
    pub fn new(system: zbus::Connection) -> Self {
        let (state_tx, state) = watch::channel(BluetoothServiceState {
            health: BluetoothServiceHealth::Starting,
            snapshot: Default::default(),
            prompt: None,
            active_action: None,
        });
        let (commands, cmd_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            run_bluetooth_service(system, state_tx, cmd_rx).await;
        });

        Self { commands, state }
    }

    pub fn subscribe(&self) -> watch::Receiver<BluetoothServiceState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: BluetoothServiceCommand,
    ) -> Result<(), mpsc::error::SendError<BluetoothServiceCommand>> {
        self.commands.send(command).await
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct OpenPopoverCount {
    count: u32,
}

impl OpenPopoverCount {
    fn open(&mut self) -> bool {
        self.count += 1;
        self.count == 1
    }

    fn close(&mut self) -> bool {
        if self.count == 0 {
            return false;
        }

        self.count -= 1;
        self.count == 0
    }

    fn has_open_popovers(&self) -> bool {
        self.count > 0
    }
}

async fn run_bluetooth_service(
    system: zbus::Connection,
    state_tx: watch::Sender<BluetoothServiceState>,
    mut cmd_rx: mpsc::Receiver<BluetoothServiceCommand>,
) {
    let provider = BluetoothProvider::new(system.clone());
    let mut attempt = 0u32;
    let mut open_popovers = OpenPopoverCount::default();

    loop {
        attempt += 1;
        let _ = state_tx.send_modify(|state| {
            state.health = if attempt == 1 {
                BluetoothServiceHealth::Starting
            } else {
                BluetoothServiceHealth::Reconnecting { attempt }
            };
        });

        match run_connected(
            system.clone(),
            provider.clone(),
            state_tx.clone(),
            &mut cmd_rx,
            &mut open_popovers,
        )
        .await
        {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "bluetooth service: worker failed");
                let _ = state_tx.send_modify(|state| {
                    state.health = BluetoothServiceHealth::Degraded {
                        message: error.to_string(),
                    };
                });
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn run_connected(
    system: zbus::Connection,
    provider: BluetoothProvider,
    state_tx: watch::Sender<BluetoothServiceState>,
    cmd_rx: &mut mpsc::Receiver<BluetoothServiceCommand>,
    open_popovers: &mut OpenPopoverCount,
) -> ServiceResult<()> {
    let (prompt_tx, _) = watch::channel(None);
    let registry = Arc::new(Mutex::new(PromptRegistry::new(prompt_tx)));
    let mut prompt_rx = registry
        .lock()
        .expect("bluetooth prompt registry poisoned")
        .subscribe();
    let agent = BluetoothAgent::new(registry.clone(), system.clone());

    agent
        .register(&system)
        .await
        .map_err(|error| -> ServiceError {
            format!("failed to register bluetooth agent: {error}").into()
        })?;

    let cancel = CancellationToken::new();
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut listener = tokio::spawn({
        let provider = provider.clone();
        let cancel = cancel.clone();
        async move { provider.listen(event_tx, cancel).await }
    });

    refresh_snapshot(&provider, &state_tx).await?;
    if open_popovers.has_open_popovers() {
        tracing::info!("bluetooth service: re-starting discovery after reconnect");
        provider.start_discovery().await?;
    }
    let _ = state_tx.send_modify(|state| state.health = BluetoothServiceHealth::Ready);

    let result = loop {
        tokio::select! {
            changed = prompt_rx.changed() => {
                if changed.is_err() {
                    break Err(service_error("bluetooth prompt stream closed"));
                }
                let prompt = prompt_rx.borrow().clone();
                let _ = state_tx.send_modify(|state| state.prompt = prompt);
            }
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(BluetoothProviderEvent::Changed { reason }) => {
                        log_provider_change(reason);
                        if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                            tracing::warn!(error = %error, "bluetooth service: refresh failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = BluetoothServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        } else {
                            let _ = state_tx.send_modify(|state| state.health = BluetoothServiceHealth::Ready);
                        }
                    }
                    None => break Err(service_error("bluetooth provider event channel closed")),
                }
            }
            maybe_command = cmd_rx.recv() => {
                match maybe_command {
                    Some(command) => {
                        handle_command(&provider, &registry, &state_tx, open_popovers, command).await?;
                    }
                    None => break Ok(()),
                }
            }
            join = &mut listener => {
                break match join {
                    Ok(Ok(())) => Err(service_error("bluetooth listener exited")),
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(service_error(format!("bluetooth listener task failed: {error}"))),
                };
            }
        }
    };

    cancel.cancel();
    let _ = agent.unregister(&system).await;
    result
}

fn log_provider_change(reason: crate::bluetooth::provider::BluetoothChangeReason) {
    tracing::debug!(reason = %reason, "bluetooth service: provider changed");
}

async fn handle_command(
    provider: &BluetoothProvider,
    registry: &Arc<Mutex<PromptRegistry>>,
    state_tx: &watch::Sender<BluetoothServiceState>,
    open_popovers: &mut OpenPopoverCount,
    command: BluetoothServiceCommand,
) -> ServiceResult<()> {
    match command {
        BluetoothServiceCommand::SetPowered(powered) => {
            spawn_action(
                provider.clone(),
                registry.clone(),
                state_tx.clone(),
                state_tx,
                Some(BluetoothActiveAction::SetPowered(powered)),
                move |provider| async move { provider.set_powered(powered).await.map_err(Into::into) },
                false,
            );
            Ok(())
        }
        BluetoothServiceCommand::SetAdapterPowered {
            adapter_path,
            powered,
        } => {
            spawn_action(
                provider.clone(),
                registry.clone(),
                state_tx.clone(),
                state_tx,
                Some(BluetoothActiveAction::SetAdapterPowered {
                    adapter_path: adapter_path.clone(),
                    powered,
                }),
                move |provider| async move {
                    provider
                        .set_adapter_powered(&adapter_path, powered)
                        .await
                        .map_err(Into::into)
                },
                false,
            );
            Ok(())
        }
        BluetoothServiceCommand::SetAdapterDiscoverable {
            adapter_path,
            discoverable,
        } => {
            spawn_action(
                provider.clone(),
                registry.clone(),
                state_tx.clone(),
                state_tx,
                Some(BluetoothActiveAction::SetAdapterDiscoverable {
                    adapter_path: adapter_path.clone(),
                    discoverable,
                }),
                move |provider| async move {
                    provider
                        .set_adapter_discoverable(&adapter_path, discoverable)
                        .await
                        .map_err(Into::into)
                },
                false,
            );
            Ok(())
        }
        BluetoothServiceCommand::StartDiscovery => {
            let needs_start = open_popovers.open();
            if needs_start {
                provider.start_discovery().await?;
            }
            Ok(())
        }
        BluetoothServiceCommand::StopDiscovery => {
            if open_popovers.has_open_popovers() {
                let needs_stop = open_popovers.close();
                if needs_stop {
                    provider.stop_discovery().await?;
                }
            }
            Ok(())
        }
        BluetoothServiceCommand::Connect { address } => {
            spawn_action(
                provider.clone(),
                registry.clone(),
                state_tx.clone(),
                state_tx,
                Some(BluetoothActiveAction::Connect {
                    address: address.clone(),
                }),
                move |provider| async move { provider.connect(&address).await.map_err(Into::into) },
                false,
            );
            Ok(())
        }
        BluetoothServiceCommand::Disconnect { address } => {
            spawn_action(
                provider.clone(),
                registry.clone(),
                state_tx.clone(),
                state_tx,
                Some(BluetoothActiveAction::Disconnect {
                    address: address.clone(),
                }),
                move |provider| async move { provider.disconnect(&address).await.map_err(Into::into) },
                false,
            );
            Ok(())
        }
        BluetoothServiceCommand::Pair { address } => {
            spawn_action(
                provider.clone(),
                registry.clone(),
                state_tx.clone(),
                state_tx,
                Some(BluetoothActiveAction::Pair {
                    address: address.clone(),
                }),
                move |provider| async move { provider.pair(&address).await.map_err(Into::into) },
                true,
            );
            Ok(())
        }
        BluetoothServiceCommand::Trust { address, trusted } => {
            spawn_action(
                provider.clone(),
                registry.clone(),
                state_tx.clone(),
                state_tx,
                Some(BluetoothActiveAction::Trust {
                    address: address.clone(),
                    trusted,
                }),
                move |provider| async move {
                    provider.trust(&address, trusted).await.map_err(Into::into)
                },
                false,
            );
            Ok(())
        }
        BluetoothServiceCommand::Forget { address } => {
            spawn_action(
                provider.clone(),
                registry.clone(),
                state_tx.clone(),
                state_tx,
                Some(BluetoothActiveAction::Forget {
                    address: address.clone(),
                }),
                move |provider| async move { provider.forget(&address).await.map_err(Into::into) },
                false,
            );
            Ok(())
        }
        BluetoothServiceCommand::PromptReply { id, reply } => {
            let handled = registry
                .lock()
                .expect("bluetooth prompt registry poisoned")
                .complete(id, reply);
            if !handled {
                tracing::warn!(
                    prompt_id = id.0,
                    "bluetooth service: prompt reply was not matched"
                );
            }
            Ok(())
        }
    }
}

fn spawn_action<F, Fut>(
    provider: BluetoothProvider,
    registry: Arc<Mutex<PromptRegistry>>,
    state_tx: watch::Sender<BluetoothServiceState>,
    state_view: &watch::Sender<BluetoothServiceState>,
    action: Option<BluetoothActiveAction>,
    make_future: F,
    clear_prompt_after: bool,
) where
    F: FnOnce(BluetoothProvider) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ServiceResult<()>> + Send + 'static,
{
    if state_view.borrow().active_action.is_some() {
        tracing::warn!(
            "bluetooth service: ignoring command while another bluetooth action is active"
        );
        return;
    }

    let _ = state_tx.send_modify(|state| state.active_action = action.clone());

    tokio::spawn(async move {
        let result = make_future(provider).await;
        if clear_prompt_after {
            let cleared = registry
                .lock()
                .expect("bluetooth prompt registry poisoned")
                .cancel_current();
            if cleared {
                tracing::info!("bluetooth service: cleared pairing prompt");
            }
        }

        if let Err(error) = &result {
            tracing::warn!(error = %error, "bluetooth service: bluetooth action failed");
        }

        let _ = state_tx.send_modify(|state| state.active_action = None);
    });
}

async fn refresh_snapshot(
    provider: &BluetoothProvider,
    state_tx: &watch::Sender<BluetoothServiceState>,
) -> ServiceResult<()> {
    let snapshot = provider.scan().await?;
    let _ = state_tx.send_modify(|state| state.snapshot = snapshot);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intermediate_popover_close_does_not_stop_discovery() {
        let mut popovers = OpenPopoverCount { count: 2 };

        assert!(!popovers.close());
        assert_eq!(popovers.count, 1);
    }

    #[test]
    fn last_popover_close_stops_discovery() {
        let mut popovers = OpenPopoverCount { count: 1 };

        assert!(popovers.close());
        assert_eq!(popovers.count, 0);
    }

    #[test]
    fn first_popover_open_starts_discovery_but_second_does_not() {
        let mut popovers = OpenPopoverCount::default();

        assert!(popovers.open());
        assert!(!popovers.open());
        assert_eq!(popovers.count, 2);
    }

    #[test]
    fn closing_without_open_popovers_is_noop() {
        let mut popovers = OpenPopoverCount::default();

        assert!(!popovers.close());
        assert_eq!(popovers.count, 0);
    }

    #[test]
    fn provider_change_logs_are_debug_only() {
        log_provider_change(crate::bluetooth::provider::BluetoothChangeReason::PropertiesChanged);
        log_provider_change(crate::bluetooth::provider::BluetoothChangeReason::Mixed);
    }
}
