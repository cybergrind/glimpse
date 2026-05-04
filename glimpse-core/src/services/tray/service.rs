use std::time::Duration;

use anyhow::{Context, anyhow};
use tokio::{
    sync::{mpsc, watch},
    time::sleep,
};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

use super::{
    model::Snapshot,
    protocol::{Command, Health, State},
    tray_client::{TrayClient, TrayClientEvent},
};

const COMMAND_QUEUE_SIZE: usize = 32;
const EVENT_QUEUE_SIZE: usize = 64;
const RETRY_DELAY: Duration = Duration::from_secs(2);

pub type TrayHandle = ServiceHandle<State, Command>;

pub struct TrayService {
    session: zbus::Connection,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

impl TrayService {
    pub fn new(session: zbus::Connection) -> (Self, TrayHandle) {
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
                    tracing::warn!(error = %error, "tray service failed");
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
        let client = TrayClient::new(self.session.clone())
            .await
            .context("failed to create tray client")?;

        self.refresh_snapshot(&client)
            .await
            .context("failed to load initial tray snapshot")?;
        self.set_health(Health::Ready);

        let (event_tx, mut event_rx) = mpsc::channel(EVENT_QUEUE_SIZE);
        let listener_cancel = CancellationToken::new();
        let listener = spawn_tray_listener(client.clone(), event_tx, listener_cancel.clone());

        let outcome = loop {
            tokio::select! {
                _ = cancel.cancelled() => break Ok(RunOutcome::Cancelled),
                event = event_rx.recv() => match event {
                    Some(TrayClientEvent::Changed { reason }) => {
                        tracing::debug!(reason = %reason, "tray: refreshing service state");
                        if let Err(error) = self.refresh_snapshot(&client).await {
                            tracing::warn!(error = %error, "tray: refresh failed after change event");
                            self.set_health(Health::Degraded {
                                message: "Tray data is stale".into(),
                            });
                        } else {
                            self.set_health(Health::Ready);
                        }
                    }
                    None => break Err(anyhow!("tray event listener stopped")),
                },
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        if let Err(error) = execute_command(&client, command).await {
                            tracing::warn!(error = %error, "tray command failed");
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

    async fn refresh_snapshot(&self, client: &TrayClient) -> anyhow::Result<()> {
        let snapshot = client.snapshot().await?;
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

async fn execute_command(client: &TrayClient, command: Command) -> anyhow::Result<()> {
    match command {
        Command::Activate { address, x, y } => client.activate(address, x, y).await,
        Command::SecondaryActivate { address, x, y } => {
            client.secondary_activate(&address, x, y).await
        }
        Command::OpenContextMenu { address, x, y } => {
            client.open_context_menu(&address, x, y).await
        }
        Command::Scroll {
            address,
            delta,
            orientation,
        } => {
            client
                .scroll(&address, delta, orientation.as_dbus_str())
                .await
        }
        Command::AboutToShowMenu {
            address,
            menu_path,
            item_id,
        } => {
            client
                .about_to_show_menu(address, menu_path, item_id)
                .await?;
            Ok(())
        }
        Command::ActivateMenuItem {
            address,
            menu_path,
            item_id,
        } => client.activate_menu_item(address, menu_path, item_id).await,
    }
}

fn spawn_tray_listener(
    client: TrayClient,
    events: mpsc::Sender<TrayClientEvent>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move { client.listen(events, cancel).await })
}

#[cfg(test)]
mod tests {
    use tokio::sync::watch;

    use super::*;
    use crate::services::tray::model::{Item, Status};

    #[test]
    fn apply_snapshot_publishes_only_changed_values() {
        let (state_tx, state_rx) = watch::channel(State::default());
        let snapshot = Snapshot {
            items: vec![Item {
                address: "org.example.App".into(),
                id: "example".into(),
                title: "Example".into(),
                status: Status::Active,
                category: Default::default(),
                item_is_menu: false,
                menu_path: String::new(),
                icon_theme_path: None,
                icon: None,
                overlay_icon: None,
                attention_icon: None,
                attention_movie_name: None,
                tooltip: None,
                menu: Vec::new(),
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
        assert!(matches!(state_rx.has_changed(), Ok(true)));
    }
}
