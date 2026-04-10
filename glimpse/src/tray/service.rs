use std::{error::Error, time::Duration};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    providers::tray::{TrayProvider, TrayProviderEvent},
    tray::protocol::{TrayServiceCommand, TrayServiceHealth, TrayServiceState},
};

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

#[derive(Clone)]
pub struct TrayServiceHandle {
    commands: mpsc::Sender<TrayServiceCommand>,
    state: watch::Receiver<TrayServiceState>,
}

impl TrayServiceHandle {
    pub fn new() -> Self {
        let (state_tx, state) = watch::channel(TrayServiceState::default());
        let (commands, cmd_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            run_tray_service(state_tx, cmd_rx).await;
        });

        Self { commands, state }
    }

    pub fn subscribe(&self) -> watch::Receiver<TrayServiceState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: TrayServiceCommand,
    ) -> Result<(), mpsc::error::SendError<TrayServiceCommand>> {
        self.commands.send(command).await
    }
}

async fn run_tray_service(
    state_tx: watch::Sender<TrayServiceState>,
    mut cmd_rx: mpsc::Receiver<TrayServiceCommand>,
) {
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        let _ = state_tx.send_modify(|state| {
            state.health = if attempt == 1 {
                TrayServiceHealth::Starting
            } else {
                TrayServiceHealth::Reconnecting { attempt }
            };
        });

        let provider = match TrayProvider::new().await {
            Ok(provider) => provider,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "tray service: failed to start provider");
                let _ = state_tx.send_modify(|state| {
                    state.health = TrayServiceHealth::Degraded {
                        message: error.to_string(),
                    };
                });
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        match run_connected(provider, state_tx.clone(), &mut cmd_rx).await {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "tray service: worker failed");
                let _ = state_tx.send_modify(|state| {
                    state.health = TrayServiceHealth::Degraded {
                        message: error.to_string(),
                    };
                });
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn run_connected(
    provider: TrayProvider,
    state_tx: watch::Sender<TrayServiceState>,
    cmd_rx: &mut mpsc::Receiver<TrayServiceCommand>,
) -> ServiceResult<()> {
    let cancel = CancellationToken::new();
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut listener = tokio::spawn({
        let provider = provider.clone();
        let cancel = cancel.clone();
        async move { provider.listen(event_tx, cancel).await }
    });

    refresh_snapshot(&provider, &state_tx).await?;
    let _ = state_tx.send_modify(|state| state.health = TrayServiceHealth::Ready);

    let result = loop {
        tokio::select! {
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(TrayProviderEvent::Changed { reason }) => {
                        tracing::info!(reason = %reason, "tray service: provider changed");
                        if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                            tracing::warn!(error = %error, "tray service: refresh failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = TrayServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        } else {
                            let _ = state_tx.send_modify(|state| state.health = TrayServiceHealth::Ready);
                        }
                    }
                    None => break Err(service_error("tray provider event channel closed")),
                }
            }
            maybe_command = cmd_rx.recv() => {
                match maybe_command {
                    Some(command) => {
                        if let Err(error) = handle_command(&provider, command).await {
                            tracing::warn!(error = %error, "tray service: command failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = TrayServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        }
                    }
                    None => break Ok(()),
                }
            }
            join = &mut listener => {
                break match join {
                    Ok(Ok(())) => Err(service_error("tray listener exited")),
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(service_error(format!("tray listener task failed: {error}"))),
                };
            }
        }
    };

    cancel.cancel();
    result
}

async fn refresh_snapshot(
    provider: &TrayProvider,
    state_tx: &watch::Sender<TrayServiceState>,
) -> ServiceResult<()> {
    let snapshot = provider.snapshot().await?;
    let _ = state_tx.send_modify(|state| state.snapshot = snapshot);
    Ok(())
}

async fn handle_command(provider: &TrayProvider, command: TrayServiceCommand) -> anyhow::Result<()> {
    match command {
        TrayServiceCommand::Activate { address, x, y } => provider.activate(address, x, y).await,
        TrayServiceCommand::OpenContextMenu { address, x, y } => {
            provider.open_context_menu(&address, x, y).await
        }
        TrayServiceCommand::AboutToShowMenu {
            address,
            menu_path,
            item_id,
        } => {
            provider
                .about_to_show_menu(address, menu_path, item_id)
                .await?;
            Ok(())
        }
        TrayServiceCommand::ActivateMenuItem {
            address,
            menu_path,
            submenu_id,
        } => provider.activate_menu_item(address, menu_path, submenu_id).await,
    }
}
