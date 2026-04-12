use std::{error::Error, time::Duration};

use tokio::sync::{mpsc, watch};

use super::protocol::{
    CompositorKind, CompositorListenerHealth, WorkspaceSnapshot, WorkspaceState, detect,
};
use crate::compositors;

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceCommand {
    Refresh,
    SwitchTo(u32),
    SwitchRelative(bool),
    FocusWindowRelative(bool),
    FocusWindow(u64),
}

#[derive(Clone)]
pub struct WorkspaceServiceHandle {
    commands: mpsc::Sender<WorkspaceCommand>,
    state: watch::Receiver<WorkspaceState>,
}

impl WorkspaceServiceHandle {
    pub fn new() -> Self {
        let compositor = detect();
        let (state_tx, state) = watch::channel(WorkspaceState {
            compositor,
            capabilities: compositor.capabilities(),
            health: if compositor == CompositorKind::Unknown {
                CompositorListenerHealth::Unsupported
            } else {
                CompositorListenerHealth::Starting
            },
            snapshot: WorkspaceSnapshot::default(),
        });
        let (commands, cmd_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            run_workspace_service(compositor, state_tx, cmd_rx).await;
        });

        Self { commands, state }
    }

    pub fn subscribe(&self) -> watch::Receiver<WorkspaceState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: WorkspaceCommand,
    ) -> Result<(), mpsc::error::SendError<WorkspaceCommand>> {
        self.commands.send(command).await
    }
}

async fn run_workspace_service(
    compositor: CompositorKind,
    state_tx: watch::Sender<WorkspaceState>,
    mut cmd_rx: mpsc::Receiver<WorkspaceCommand>,
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

        match run_connected(compositor, state_tx.clone(), &mut cmd_rx).await {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "workspace service: worker failed");
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
    state_tx: watch::Sender<WorkspaceState>,
    cmd_rx: &mut mpsc::Receiver<WorkspaceCommand>,
) -> ServiceResult<()> {
    let (event_tx, mut event_rx) = mpsc::channel::<()>(16);
    let mut listener = tokio::spawn(async move {
        match compositor {
            CompositorKind::Hyprland => compositors::hyprland::workspace_event_loop(event_tx).await,
            CompositorKind::Niri => compositors::niri::workspace_event_loop(event_tx).await,
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
                    None => break Err(service_error("workspace event channel closed")),
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
                    Ok(Ok(())) => Err(service_error("workspace listener exited")),
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(service_error(format!("workspace listener task failed: {error}"))),
                };
            }
        }
    };

    listener.abort();
    result
}

async fn handle_command(
    compositor: CompositorKind,
    state_tx: &watch::Sender<WorkspaceState>,
    command: WorkspaceCommand,
) -> ServiceResult<()> {
    match command {
        WorkspaceCommand::Refresh => refresh_snapshot(compositor, state_tx).await,
        WorkspaceCommand::SwitchTo(index) => {
            match compositor {
                CompositorKind::Hyprland => compositors::hyprland::switch_workspace(index).await,
                CompositorKind::Niri => compositors::niri::switch_workspace(index).await,
                CompositorKind::Unknown => {}
            }
            refresh_snapshot(compositor, state_tx).await
        }
        WorkspaceCommand::SwitchRelative(next) => {
            match compositor {
                CompositorKind::Hyprland => {
                    compositors::hyprland::switch_workspace_relative(next).await
                }
                CompositorKind::Niri => compositors::niri::switch_workspace_relative(next).await,
                CompositorKind::Unknown => {}
            }
            refresh_snapshot(compositor, state_tx).await
        }
        WorkspaceCommand::FocusWindowRelative(next) => {
            if compositor == CompositorKind::Niri {
                compositors::niri::focus_window_relative(next).await;
            }
            refresh_snapshot(compositor, state_tx).await
        }
        WorkspaceCommand::FocusWindow(id) => {
            if compositor == CompositorKind::Niri {
                compositors::niri::focus_window(id).await;
            }
            refresh_snapshot(compositor, state_tx).await
        }
    }
}

async fn refresh_snapshot(
    compositor: CompositorKind,
    state_tx: &watch::Sender<WorkspaceState>,
) -> ServiceResult<()> {
    let snapshot = match compositor {
        CompositorKind::Hyprland => compositors::hyprland::workspace_snapshot().await,
        CompositorKind::Niri => compositors::niri::workspace_snapshot().await,
        CompositorKind::Unknown => Some(WorkspaceSnapshot::default()),
    }
    .ok_or_else(|| service_error("failed to query compositor workspace state"))?;

    state_tx.send_modify(|state| state.snapshot = snapshot);
    Ok(())
}
