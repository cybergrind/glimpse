use std::{error::Error, time::Duration};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    mpris::protocol::{MprisServiceCommand, MprisServiceHealth, MprisServiceState},
    providers::mpris::{MprisProvider, MprisProviderEvent},
};

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

#[derive(Clone)]
pub struct MprisServiceHandle {
    commands: mpsc::Sender<MprisServiceCommand>,
    state: watch::Receiver<MprisServiceState>,
}

impl MprisServiceHandle {
    pub fn new(session: zbus::Connection) -> Self {
        let (state_tx, state) = watch::channel(MprisServiceState::default());
        let (commands, cmd_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            run_mpris_service(session, state_tx, cmd_rx).await;
        });

        Self { commands, state }
    }

    pub fn subscribe(&self) -> watch::Receiver<MprisServiceState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: MprisServiceCommand,
    ) -> Result<(), mpsc::error::SendError<MprisServiceCommand>> {
        self.commands.send(command).await
    }
}

async fn run_mpris_service(
    session: zbus::Connection,
    state_tx: watch::Sender<MprisServiceState>,
    mut cmd_rx: mpsc::Receiver<MprisServiceCommand>,
) {
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        let _ = state_tx.send_modify(|state| {
            state.health = if attempt == 1 {
                MprisServiceHealth::Starting
            } else {
                MprisServiceHealth::Reconnecting { attempt }
            };
        });

        let provider = match MprisProvider::new(session.clone()).await {
            Ok(provider) => provider,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "mpris service: failed to start provider");
                let _ = state_tx.send_modify(|state| {
                    state.health = MprisServiceHealth::Degraded {
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
                tracing::warn!(error = %error, attempt, "mpris service: worker failed");
                let _ = state_tx.send_modify(|state| {
                    state.health = MprisServiceHealth::Degraded {
                        message: error.to_string(),
                    };
                });
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn run_connected(
    provider: MprisProvider,
    state_tx: watch::Sender<MprisServiceState>,
    cmd_rx: &mut mpsc::Receiver<MprisServiceCommand>,
) -> ServiceResult<()> {
    let cancel = CancellationToken::new();
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut listener = tokio::spawn({
        let provider = provider.clone();
        let cancel = cancel.clone();
        async move { provider.listen(event_tx, cancel).await }
    });

    refresh_snapshot(&provider, &state_tx).await?;
    let _ = state_tx.send_modify(|state| state.health = MprisServiceHealth::Ready);

    let result = loop {
        tokio::select! {
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(MprisProviderEvent::Changed { reason }) => {
                        tracing::debug!(reason = %reason, "mpris service: provider changed");
                        if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                            tracing::warn!(error = %error, "mpris service: refresh failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = MprisServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        } else {
                            let _ = state_tx.send_modify(|state| state.health = MprisServiceHealth::Ready);
                        }
                    }
                    None => break Err(service_error("mpris provider event channel closed")),
                }
            }
            maybe_command = cmd_rx.recv() => {
                match maybe_command {
                    Some(command) => {
                        if let Err(error) = handle_command(&provider, command).await {
                            tracing::warn!(error = %error, "mpris service: command failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = MprisServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                            continue;
                        }

                        if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                            tracing::warn!(error = %error, "mpris service: refresh failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = MprisServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        } else {
                            let _ = state_tx.send_modify(|state| state.health = MprisServiceHealth::Ready);
                        }
                    }
                    None => break Ok(()),
                }
            }
            join = &mut listener => {
                break match join {
                    Ok(Ok(())) => Err(service_error("mpris listener exited")),
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(service_error(format!("mpris listener task failed: {error}"))),
                };
            }
        }
    };

    cancel.cancel();
    result
}

async fn refresh_snapshot(
    provider: &MprisProvider,
    state_tx: &watch::Sender<MprisServiceState>,
) -> ServiceResult<()> {
    provider.refresh().await?;
    let snapshot = provider.snapshot();
    let _ = state_tx.send_modify(|state| state.snapshot = snapshot);
    Ok(())
}

async fn handle_command(
    provider: &MprisProvider,
    command: MprisServiceCommand,
) -> anyhow::Result<()> {
    match command {
        MprisServiceCommand::PlayPause { player_id } => provider.play_pause(&player_id).await,
        MprisServiceCommand::Previous { player_id } => provider.previous(&player_id).await,
        MprisServiceCommand::Next { player_id } => provider.next(&player_id).await,
        MprisServiceCommand::Raise { player_id } => provider.raise(&player_id).await,
        MprisServiceCommand::Refresh => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mpris::protocol::{MprisServiceCommand, MprisServiceHealth};
    use tokio::time::{Duration, timeout};

    async fn session_connection() -> Option<zbus::Connection> {
        match zbus::Connection::session().await {
            Ok(connection) => Some(connection),
            Err(error) => {
                eprintln!("skipping mpris service test without session bus: {error}");
                None
            }
        }
    }

    #[tokio::test]
    async fn handle_new_exposes_cloneable_watch_receivers() {
        let Some(session) = session_connection().await else {
            return;
        };

        let handle = MprisServiceHandle::new(session);
        let first = handle.subscribe();
        let second = handle.subscribe();

        assert_eq!(*first.borrow(), *second.borrow());
        assert!(matches!(
            first.borrow().health,
            MprisServiceHealth::Starting
                | MprisServiceHealth::Ready
                | MprisServiceHealth::Degraded { .. }
                | MprisServiceHealth::Reconnecting { .. }
        ));
    }

    #[tokio::test]
    async fn refresh_command_triggers_a_state_publication() {
        let Some(session) = session_connection().await else {
            return;
        };

        let handle = MprisServiceHandle::new(session);
        let mut state = handle.subscribe();

        timeout(Duration::from_secs(2), state.changed())
            .await
            .expect("service should publish an initial state update")
            .expect("service state channel should stay open");
        let _ = state.borrow_and_update();

        handle
            .send(MprisServiceCommand::Refresh)
            .await
            .expect("refresh command should be accepted");

        timeout(Duration::from_secs(2), state.changed())
            .await
            .expect("refresh command should trigger a state publication")
            .expect("service state channel should stay open");
    }
}
