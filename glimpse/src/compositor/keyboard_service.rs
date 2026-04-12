use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::sync::{mpsc, watch};

use super::protocol::{
    CompositorKind, CompositorListenerHealth, KeyboardLayoutSnapshot, KeyboardLayoutState, detect,
};
use crate::compositors;

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyboardLayoutCommand {
    Refresh,
    SwitchRelative(bool),
}

#[derive(Clone)]
pub struct KeyboardLayoutServiceHandle {
    commands: mpsc::Sender<KeyboardLayoutCommand>,
    state: watch::Receiver<KeyboardLayoutState>,
    per_window: Arc<AtomicBool>,
}

impl KeyboardLayoutServiceHandle {
    pub fn new() -> Self {
        Self::new_with_per_window(false)
    }

    pub fn new_with_per_window(per_window_enabled: bool) -> Self {
        let compositor = detect();
        let (state_tx, state) = watch::channel(KeyboardLayoutState {
            compositor,
            capabilities: compositor.capabilities(),
            health: if compositor == CompositorKind::Unknown {
                CompositorListenerHealth::Unsupported
            } else {
                CompositorListenerHealth::Starting
            },
            snapshot: KeyboardLayoutSnapshot::default(),
        });
        let (commands, cmd_rx) = mpsc::channel(64);
        let per_window = Arc::new(AtomicBool::new(per_window_enabled));
        let listener_per_window = per_window.clone();

        tokio::spawn(async move {
            run_keyboard_service(compositor, listener_per_window, state_tx, cmd_rx).await;
        });

        Self {
            commands,
            state,
            per_window,
        }
    }

    pub fn fork(&self, per_window_enabled: bool) -> Self {
        Self::new_with_per_window(per_window_enabled)
    }

    pub fn per_window_enabled(&self) -> bool {
        self.per_window.load(Ordering::Relaxed)
    }

    pub fn subscribe(&self) -> watch::Receiver<KeyboardLayoutState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: KeyboardLayoutCommand,
    ) -> Result<(), mpsc::error::SendError<KeyboardLayoutCommand>> {
        self.commands.send(command).await
    }
}

async fn run_keyboard_service(
    compositor: CompositorKind,
    per_window: Arc<AtomicBool>,
    state_tx: watch::Sender<KeyboardLayoutState>,
    mut cmd_rx: mpsc::Receiver<KeyboardLayoutCommand>,
) {
    if compositor == CompositorKind::Unknown {
        return;
    }

    let mut attempt = 0u32;
    loop {
        attempt += 1;
        state_tx.send_modify(|state| {
            state.health = if attempt == 1 {
                CompositorListenerHealth::Starting
            } else {
                CompositorListenerHealth::Reconnecting { attempt }
            };
        });

        match run_connected(
            compositor,
            per_window.clone(),
            state_tx.clone(),
            &mut cmd_rx,
        )
        .await
        {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "keyboard layout service: worker failed");
                state_tx.send_modify(|state| {
                    state.health = CompositorListenerHealth::Degraded {
                        message: error.to_string(),
                    };
                });
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn run_connected(
    compositor: CompositorKind,
    per_window: Arc<AtomicBool>,
    state_tx: watch::Sender<KeyboardLayoutState>,
    cmd_rx: &mut mpsc::Receiver<KeyboardLayoutCommand>,
) -> ServiceResult<()> {
    let (event_tx, mut event_rx) = mpsc::channel::<()>(16);
    let listener_per_window = per_window.clone();
    let mut listener = tokio::spawn(async move {
        match compositor {
            CompositorKind::Hyprland => {
                compositors::hyprland::keyboard_event_loop(event_tx, listener_per_window.clone())
                    .await
            }
            CompositorKind::Niri => {
                compositors::niri::keyboard_event_loop(event_tx, listener_per_window.clone()).await
            }
            CompositorKind::Unknown => Ok(()),
        }
    });

    refresh_snapshot(compositor, &state_tx).await?;
    state_tx.send_modify(|state| state.health = CompositorListenerHealth::Ready);

    let result = loop {
        tokio::select! {
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(()) => {
                        refresh_snapshot(compositor, &state_tx).await?;
                        state_tx.send_modify(|state| state.health = CompositorListenerHealth::Ready);
                    }
                    None => break Err(service_error("keyboard layout event channel closed")),
                }
            }
            maybe_command = cmd_rx.recv() => {
                match maybe_command {
                    Some(command) => handle_command(compositor, &state_tx, command).await?,
                    None => break Ok(()),
                }
            }
            join = &mut listener => {
                break match join {
                    Ok(Ok(())) => Err(service_error("keyboard layout listener exited")),
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(service_error(format!("keyboard layout listener task failed: {error}"))),
                };
            }
        }
    };

    listener.abort();
    result
}

async fn handle_command(
    compositor: CompositorKind,
    state_tx: &watch::Sender<KeyboardLayoutState>,
    command: KeyboardLayoutCommand,
) -> ServiceResult<()> {
    match command {
        KeyboardLayoutCommand::Refresh => refresh_snapshot(compositor, state_tx).await,
        KeyboardLayoutCommand::SwitchRelative(next) => {
            match compositor {
                CompositorKind::Hyprland => {
                    compositors::hyprland::switch_layout_relative(next).await
                }
                CompositorKind::Niri => compositors::niri::switch_layout_relative(next).await,
                CompositorKind::Unknown => {}
            }
            refresh_snapshot(compositor, state_tx).await
        }
    }
}

async fn refresh_snapshot(
    compositor: CompositorKind,
    state_tx: &watch::Sender<KeyboardLayoutState>,
) -> ServiceResult<()> {
    let snapshot = match compositor {
        CompositorKind::Hyprland => compositors::hyprland::keyboard_snapshot().await,
        CompositorKind::Niri => compositors::niri::keyboard_snapshot().await,
        CompositorKind::Unknown => Some(KeyboardLayoutSnapshot::default()),
    }
    .ok_or_else(|| service_error("failed to query compositor keyboard layout state"))?;

    state_tx.send_modify(|state| state.snapshot = snapshot);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::KeyboardLayoutServiceHandle;
    use std::sync::Arc;

    #[tokio::test]
    async fn fork_creates_independent_handles_with_requested_per_window_mode() {
        let base = KeyboardLayoutServiceHandle::new();
        let per_window = base.fork(true);
        let not_per_window = base.fork(false);

        assert!(!base.per_window_enabled());
        assert!(per_window.per_window_enabled());
        assert!(!not_per_window.per_window_enabled());
        assert!(!Arc::ptr_eq(
            &per_window.per_window,
            &not_per_window.per_window
        ));
    }
}
