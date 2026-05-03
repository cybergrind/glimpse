use std::time::Duration;

use anyhow::{Context, anyhow};
use tokio::{
    sync::{mpsc, watch},
    time::{MissedTickBehavior, interval, sleep},
};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

use super::{
    model::{Command, Health, Snapshot, State},
    mpris_client::{MprisClient, MprisClientEvent},
};

const COMMAND_QUEUE_SIZE: usize = 32;
const EVENT_QUEUE_SIZE: usize = 32;
const RETRY_DELAY: Duration = Duration::from_secs(2);

pub type MprisHandle = ServiceHandle<State, Command>;

pub struct MprisService {
    session: zbus::Connection,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

impl MprisService {
    pub fn new(session: zbus::Connection) -> (Self, MprisHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                session,
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
                    tracing::warn!(error = %error, "mpris service failed");
                    self.set_health(Health::Reconnecting {
                        attempt: reconnect_attempt,
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
        self.set_health(Health::Starting);
        let client = MprisClient::new(self.session.clone())
            .await
            .context("failed to create MPRIS client")?;

        self.refresh_snapshot(&client)
            .await
            .context("failed to load initial MPRIS snapshot")?;
        self.set_health(Health::Ready);

        let (event_tx, mut event_rx) = mpsc::channel(EVENT_QUEUE_SIZE);
        let listener_cancel = CancellationToken::new();
        let listener = spawn_mpris_listener(client.clone(), event_tx, listener_cancel.clone());
        let mut progress_refresh = interval(Duration::from_secs(1));
        progress_refresh.set_missed_tick_behavior(MissedTickBehavior::Skip);

        let outcome = loop {
            tokio::select! {
                _ = cancel.cancelled() => break Ok(RunOutcome::Cancelled),
                event = event_rx.recv() => match event {
                    Some(MprisClientEvent::Changed { reason }) => {
                        tracing::debug!(reason = %reason, "mpris: refreshing service state");
                        if let Err(error) = self.refresh_snapshot(&client).await {
                            tracing::warn!(error = %error, "mpris: refresh failed after change event");
                            self.set_health(Health::Degraded {
                                message: "MPRIS data is stale".into(),
                            });
                        } else {
                            self.set_health(Health::Ready);
                        }
                    }
                    None => break Err(anyhow!("mpris event listener stopped")),
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        if let Err(error) = execute_command(&client, command).await {
                            tracing::warn!(error = %error, "mpris command failed");
                            self.set_health(Health::Degraded {
                                message: error.to_string(),
                            });
                        } else if let Err(error) = self.refresh_snapshot(&client).await {
                            tracing::warn!(error = %error, "mpris refresh failed after command");
                        } else {
                            self.set_health(Health::Ready);
                        }
                    }
                    Some(ServiceCommand::Control(Control::Start(_)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(_))) => {}
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        break Ok(RunOutcome::Cancelled);
                    }
                },
                _ = progress_refresh.tick(), if should_refresh_progress(&client.snapshot()) => {
                    match client.refresh_positions().await {
                        Ok(snapshot) => self.apply_snapshot(snapshot),
                        Err(error) => {
                            tracing::debug!(error = %error, "mpris progress refresh failed");
                        }
                    }
                }
            }
        };

        listener_cancel.cancel();
        let _ = listener.await;
        outcome
    }

    async fn refresh_snapshot(&self, client: &MprisClient) -> anyhow::Result<()> {
        let snapshot = client.refresh().await?;
        self.apply_snapshot(snapshot);
        Ok(())
    }

    fn apply_snapshot(&self, snapshot: Snapshot) {
        let _ = self.state_tx.send_if_modified(|state| {
            if state.snapshot == snapshot {
                return false;
            }
            state.snapshot = snapshot.clone();
            true
        });
    }

    fn set_health(&self, health: Health) {
        let _ = self.state_tx.send_if_modified(|state| {
            if state.health == health {
                return false;
            }
            state.health = health.clone();
            true
        });
    }
}

fn should_refresh_progress(snapshot: &Snapshot) -> bool {
    snapshot.players.iter().any(|player| {
        player.playback_status == super::model::PlaybackStatus::Playing && player.progress_visible
    })
}

async fn execute_command(client: &MprisClient, command: Command) -> anyhow::Result<()> {
    match command {
        Command::PlayPause { player_id } => client.play_pause(&player_id).await,
        Command::Previous { player_id } => client.previous(&player_id).await,
        Command::Next { player_id } => client.next(&player_id).await,
        Command::Raise { player_id } => client.raise(&player_id).await,
    }
}

fn spawn_mpris_listener(
    client: MprisClient,
    events: mpsc::Sender<MprisClientEvent>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move { client.listen(events, cancel).await })
}

#[cfg(test)]
mod tests {
    use tokio::sync::watch;

    use super::*;
    use crate::services::mpris::model::{PlaybackStatus, Player};

    #[test]
    fn apply_snapshot_publishes_only_changed_values() {
        let (state_tx, state_rx) = watch::channel(State::default());
        let snapshot = Snapshot {
            current_player: None,
            players: vec![Player {
                player_id: "spotify".into(),
                playback_status: PlaybackStatus::Playing,
                ..Default::default()
            }],
        };

        let first = state_tx.send_if_modified(|state| {
            if state.snapshot == snapshot {
                return false;
            }
            state.snapshot = snapshot.clone();
            true
        });
        let second = state_tx.send_if_modified(|state| {
            if state.snapshot == snapshot {
                return false;
            }
            state.snapshot = snapshot.clone();
            true
        });

        assert!(first);
        assert!(!second);
        assert_eq!(state_rx.borrow().snapshot, snapshot);
    }

    #[test]
    fn progress_refresh_runs_only_for_playing_progress_players() {
        assert!(!should_refresh_progress(&Snapshot::default()));
        assert!(should_refresh_progress(&Snapshot {
            current_player: None,
            players: vec![Player {
                player_id: "spotify".into(),
                playback_status: PlaybackStatus::Playing,
                progress_visible: true,
                ..Default::default()
            }],
        }));
        assert!(!should_refresh_progress(&Snapshot {
            current_player: None,
            players: vec![Player {
                player_id: "spotify".into(),
                playback_status: PlaybackStatus::Paused,
                progress_visible: true,
                ..Default::default()
            }],
        }));
    }
}
