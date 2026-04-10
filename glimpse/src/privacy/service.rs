use std::{sync::Arc, time::Duration};

use tokio::sync::{Mutex, mpsc, watch};

use crate::{
    privacy::protocol::{
        PrivacyServiceCommand, PrivacyServiceHealth, PrivacyServiceState,
    },
    providers::privacy::{PrivacyBackend, PrivacyProvider},
};

#[derive(Clone)]
pub struct PrivacyServiceHandle {
    commands: mpsc::Sender<PrivacyServiceCommand>,
    state: watch::Receiver<PrivacyServiceState>,
}

impl PrivacyServiceHandle {
    pub fn new(session: zbus::Connection) -> Self {
        let (state_tx, state) = watch::channel(PrivacyServiceState::default());
        let (commands, cmd_rx) = mpsc::channel(32);

        tokio::spawn(async move {
            run_privacy_service(Box::new(PrivacyProvider::new(session)), state_tx, cmd_rx).await;
        });

        Self { commands, state }
    }

    #[cfg(test)]
    fn from_backend(backend: Box<dyn PrivacyBackend>) -> Self {
        let (state_tx, state) = watch::channel(PrivacyServiceState::default());
        let (commands, cmd_rx) = mpsc::channel(32);

        tokio::spawn(async move {
            run_privacy_service(backend, state_tx, cmd_rx).await;
        });

        Self { commands, state }
    }

    pub fn subscribe(&self) -> watch::Receiver<PrivacyServiceState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: PrivacyServiceCommand,
    ) -> Result<(), mpsc::error::SendError<PrivacyServiceCommand>> {
        self.commands.send(command).await
    }
}

async fn run_privacy_service(
    backend: Box<dyn PrivacyBackend>,
    state_tx: watch::Sender<PrivacyServiceState>,
    mut cmd_rx: mpsc::Receiver<PrivacyServiceCommand>,
) {
    let backend = Arc::new(Mutex::new(backend));

    if let Err(error) = refresh_state(&backend, &state_tx).await {
        let _ = state_tx.send_modify(|state| {
            state.health = PrivacyServiceHealth::Degraded {
                message: error.to_string(),
            };
        });
    }

    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(error) = refresh_state(&backend, &state_tx).await {
                    tracing::warn!(error = %error, "privacy service: snapshot refresh failed");
                    let _ = state_tx.send_modify(|state| {
                        state.health = PrivacyServiceHealth::Degraded {
                            message: error.to_string(),
                        };
                    });
                }
            }
            maybe_command = cmd_rx.recv() => {
                let Some(command) = maybe_command else {
                    break;
                };

                if let Err(error) = handle_command(&backend, &state_tx, command).await {
                    tracing::warn!(error = %error, "privacy service: command failed");
                    let _ = state_tx.send_modify(|state| {
                        state.health = PrivacyServiceHealth::Degraded {
                            message: error.to_string(),
                        };
                    });
                }
            }
        }
    }
}

async fn refresh_state(
    backend: &Arc<Mutex<Box<dyn PrivacyBackend>>>,
    state_tx: &watch::Sender<PrivacyServiceState>,
) -> anyhow::Result<()> {
    let snapshot = {
        let mut backend = backend.lock().await;
        backend.snapshot().await?
    };

    let _ = state_tx.send_modify(|state| {
        state.snapshot = snapshot.clone();
        state.health = PrivacyServiceHealth::Ready;
    });
    Ok(())
}

async fn handle_command(
    backend: &Arc<Mutex<Box<dyn PrivacyBackend>>>,
    state_tx: &watch::Sender<PrivacyServiceState>,
    command: PrivacyServiceCommand,
) -> anyhow::Result<()> {
    {
        let mut backend = backend.lock().await;
        match command {
            PrivacyServiceCommand::StopAllScreenCapture => {
                backend.stop_all_screen_capture().await?;
            }
            PrivacyServiceCommand::StopSession { session_id } => {
                backend.stop_session(&session_id).await?;
            }
            PrivacyServiceCommand::Refresh => {}
        }
    }

    refresh_state(backend, state_tx).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;

    use super::*;
    use crate::privacy::protocol::{PrivacyIndicatorSnapshot, PrivacySession, PrivacySessionKind};

    #[derive(Default)]
    struct MockBackend {
        snapshot: PrivacyIndicatorSnapshot,
        stopped_all: usize,
        stopped_sessions: StdMutex<Vec<String>>,
    }

    #[async_trait]
    impl PrivacyBackend for MockBackend {
        async fn snapshot(&mut self) -> anyhow::Result<PrivacyIndicatorSnapshot> {
            Ok(self.snapshot.clone())
        }

        async fn stop_all_screen_capture(&mut self) -> anyhow::Result<()> {
            self.stopped_all += 1;
            self.snapshot.screen_capture_active = false;
            Ok(())
        }

        async fn stop_session(&mut self, session_id: &str) -> anyhow::Result<()> {
            self.stopped_sessions
                .lock()
                .unwrap()
                .push(session_id.to_string());
            self.snapshot.sessions.retain(|session| session.session_id != session_id);
            self.snapshot.screen_capture_active = self
                .snapshot
                .sessions
                .iter()
                .any(|session| matches!(session.kind, Some(PrivacySessionKind::ScreenCapture)));
            Ok(())
        }
    }

    #[tokio::test]
    async fn stop_session_command_refreshes_snapshot() {
        let backend = MockBackend {
            snapshot: PrivacyIndicatorSnapshot {
                screen_capture_active: true,
                sessions: vec![PrivacySession {
                    session_id: "capture-1".into(),
                    app_name: "Firefox".into(),
                    backend: "pipewire".into(),
                    started_at: Some(10),
                    stoppable: true,
                    supported_action: None,
                    kind: Some(PrivacySessionKind::ScreenCapture),
                }],
                ..Default::default()
            },
            ..Default::default()
        };

        let handle = PrivacyServiceHandle::from_backend(Box::new(backend));
        let mut state = handle.subscribe();
        wait_for_state(&mut state, |state| state.snapshot.screen_capture_active).await;

        handle
            .send(PrivacyServiceCommand::StopSession {
                session_id: "capture-1".into(),
            })
            .await
            .unwrap();
        wait_for_state(&mut state, |state| !state.snapshot.screen_capture_active).await;

        assert!(!state.borrow().snapshot.screen_capture_active);
        assert!(state.borrow().snapshot.sessions.is_empty());
    }

    async fn wait_for_state(
        state: &mut watch::Receiver<PrivacyServiceState>,
        predicate: impl Fn(&PrivacyServiceState) -> bool,
    ) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
        loop {
            if predicate(&state.borrow()) {
                break;
            }

            tokio::time::timeout_at(deadline, state.changed())
                .await
                .expect("timed out waiting for privacy state")
                .expect("privacy state channel closed");
        }
    }
}
