use std::{error::Error, time::Duration};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    tray::protocol::{TrayServiceCommand, TrayServiceHealth, TrayServiceState},
    tray::provider::{TrayProvider, TrayProviderEvent},
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
        set_health_if_changed(
            &state_tx,
            if attempt == 1 {
                TrayServiceHealth::Starting
            } else {
                TrayServiceHealth::Reconnecting { attempt }
            },
        );

        let provider = match TrayProvider::new().await {
            Ok(provider) => provider,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "tray service: failed to start provider");
                set_health_if_changed(
                    &state_tx,
                    TrayServiceHealth::Degraded {
                        message: error.to_string(),
                    },
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
        };

        match run_connected(provider, state_tx.clone(), &mut cmd_rx).await {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "tray service: worker failed");
                set_health_if_changed(
                    &state_tx,
                    TrayServiceHealth::Degraded {
                        message: error.to_string(),
                    },
                );
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
    set_health_if_changed(&state_tx, TrayServiceHealth::Ready);

    let result = loop {
        tokio::select! {
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(TrayProviderEvent::Changed { reason }) => {
                        tracing::debug!(reason = %reason, "tray service: provider changed");
                        if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                            tracing::warn!(error = %error, "tray service: refresh failed");
                            set_health_if_changed(
                                &state_tx,
                                TrayServiceHealth::Degraded {
                                    message: error.to_string(),
                                },
                            );
                        } else {
                            set_health_if_changed(&state_tx, TrayServiceHealth::Ready);
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
                            set_health_if_changed(
                                &state_tx,
                                TrayServiceHealth::Degraded {
                                    message: error.to_string(),
                                },
                            );
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
    apply_snapshot_if_changed(state_tx, snapshot);
    Ok(())
}

fn apply_snapshot_if_changed(
    state_tx: &watch::Sender<TrayServiceState>,
    next_snapshot: crate::tray::protocol::TraySnapshot,
) {
    let _ = state_tx.send_if_modified(|state| {
        if state.snapshot == next_snapshot {
            return false;
        }
        state.snapshot = next_snapshot.clone();
        true
    });
}

fn set_health_if_changed(
    state_tx: &watch::Sender<TrayServiceState>,
    next_health: TrayServiceHealth,
) {
    let _ = state_tx.send_if_modified(|state| {
        if state.health == next_health {
            return false;
        }
        state.health = next_health.clone();
        true
    });
}

async fn handle_command(
    provider: &TrayProvider,
    command: TrayServiceCommand,
) -> anyhow::Result<()> {
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
        } => {
            provider
                .activate_menu_item(address, menu_path, submenu_id)
                .await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{sync::watch, time::timeout};

    use super::{apply_snapshot_if_changed, set_health_if_changed};
    use crate::tray::protocol::{
        TrayItem, TrayServiceHealth, TrayServiceState, TraySnapshot, TrayStatus,
    };

    #[tokio::test]
    async fn apply_snapshot_if_changed_only_notifies_when_snapshot_changes() {
        let (state_tx, mut state_rx) = watch::channel(TrayServiceState::default());
        let snapshot = TraySnapshot {
            items: vec![test_item("org.example.App")],
        };

        apply_snapshot_if_changed(&state_tx, snapshot.clone());
        timeout(Duration::from_millis(20), state_rx.changed())
            .await
            .expect("first snapshot change should notify")
            .unwrap();
        state_rx.borrow_and_update();

        apply_snapshot_if_changed(&state_tx, snapshot);
        assert!(
            timeout(Duration::from_millis(20), state_rx.changed())
                .await
                .is_err()
        );
    }

    #[test]
    fn set_health_if_changed_skips_identical_health_values() {
        let (state_tx, state_rx) = watch::channel(TrayServiceState {
            health: TrayServiceHealth::Ready,
            snapshot: TraySnapshot::default(),
        });

        let version = state_rx.borrow().clone();
        set_health_if_changed(&state_tx, TrayServiceHealth::Ready);
        assert_eq!(*state_rx.borrow(), version);
    }

    fn test_item(address: &str) -> TrayItem {
        TrayItem {
            address: address.into(),
            id: "demo".into(),
            title: "Demo".into(),
            status: TrayStatus::Active,
            category: crate::tray::protocol::TrayCategory::ApplicationStatus,
            item_is_menu: false,
            menu_path: String::new(),
            icon_theme_path: None,
            icon: None,
            overlay_icon: None,
            attention_icon: None,
            attention_movie_name: None,
            tooltip: None,
            menu: Vec::new(),
        }
    }
}
