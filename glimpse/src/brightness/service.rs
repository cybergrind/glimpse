use std::{error::Error, time::Duration};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    brightness::protocol::{
        BrightnessActiveAdjustment, BrightnessServiceCommand, BrightnessServiceHealth,
        BrightnessServiceState,
    },
    brightness::provider::{BrightnessProvider, BrightnessProviderEvent},
};

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

#[derive(Clone)]
pub struct BrightnessServiceHandle {
    commands: mpsc::Sender<BrightnessServiceCommand>,
    state: watch::Receiver<BrightnessServiceState>,
}

impl BrightnessServiceHandle {
    pub fn new(system: zbus::Connection) -> Self {
        let (state_tx, state) = watch::channel(BrightnessServiceState {
            health: BrightnessServiceHealth::Starting,
            snapshot: Default::default(),
            active_adjustment: None,
        });
        let (commands, cmd_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            run_brightness_service(system, state_tx, cmd_rx).await;
        });

        Self { commands, state }
    }

    pub fn subscribe(&self) -> watch::Receiver<BrightnessServiceState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: BrightnessServiceCommand,
    ) -> Result<(), mpsc::error::SendError<BrightnessServiceCommand>> {
        self.commands.send(command).await
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct OpenPopoverCount {
    count: u32,
}

impl OpenPopoverCount {
    fn open(&mut self) {
        self.count += 1;
    }

    fn close(&mut self) {
        self.count = self.count.saturating_sub(1);
    }
}

async fn run_brightness_service(
    system: zbus::Connection,
    state_tx: watch::Sender<BrightnessServiceState>,
    mut cmd_rx: mpsc::Receiver<BrightnessServiceCommand>,
) {
    let provider = BrightnessProvider::new(system);
    let mut attempt = 0u32;

    loop {
        attempt += 1;
        state_tx.send_modify(|state| {
            state.health = if attempt == 1 {
                BrightnessServiceHealth::Starting
            } else {
                BrightnessServiceHealth::Reconnecting { attempt }
            };
        });

        match run_connected(provider.clone(), state_tx.clone(), &mut cmd_rx).await {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "brightness service: worker failed");
                state_tx.send_modify(|state| {
                    state.health = BrightnessServiceHealth::Degraded {
                        message: error.to_string(),
                    };
                });
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn run_connected(
    provider: BrightnessProvider,
    state_tx: watch::Sender<BrightnessServiceState>,
    cmd_rx: &mut mpsc::Receiver<BrightnessServiceCommand>,
) -> ServiceResult<()> {
    let cancel = CancellationToken::new();
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut listener = tokio::spawn({
        let provider = provider.clone();
        let cancel = cancel.clone();
        async move { provider.listen(event_tx, cancel).await }
    });
    let mut open_popovers = OpenPopoverCount::default();

    refresh_snapshot(&provider, &state_tx).await?;
    state_tx.send_modify(|state| state.health = BrightnessServiceHealth::Ready);

    let result = loop {
        tokio::select! {
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(BrightnessProviderEvent::Changed { reason }) => {
                        tracing::debug!(reason = %reason, "brightness service: provider changed");
                        if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                            tracing::warn!(error = %error, "brightness service: refresh failed");
                            state_tx.send_modify(|state| {
                                state.health = BrightnessServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        } else {
                            state_tx.send_modify(|state| state.health = BrightnessServiceHealth::Ready);
                        }
                    }
                    None => break Err(service_error("brightness provider event channel closed")),
                }
            }
            maybe_command = cmd_rx.recv() => {
                match maybe_command {
                    Some(command) => handle_command(&provider, &state_tx, &mut open_popovers, command).await?,
                    None => break Ok(()),
                }
            }
            join = &mut listener => {
                break match join {
                    Ok(Ok(())) => Err(service_error("brightness listener exited")),
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(service_error(format!("brightness listener task failed: {error}"))),
                };
            }
        }
    };

    cancel.cancel();
    result
}

async fn handle_command(
    provider: &BrightnessProvider,
    state_tx: &watch::Sender<BrightnessServiceState>,
    open_popovers: &mut OpenPopoverCount,
    command: BrightnessServiceCommand,
) -> ServiceResult<()> {
    match command {
        BrightnessServiceCommand::Refresh => refresh_snapshot(provider, state_tx).await,
        BrightnessServiceCommand::PopoverOpened => {
            open_popovers.open();
            refresh_snapshot(provider, state_tx).await
        }
        BrightnessServiceCommand::PopoverClosed => {
            open_popovers.close();
            refresh_snapshot(provider, state_tx).await
        }
        BrightnessServiceCommand::SetDisplayPercent {
            display_id,
            percent,
        } => {
            let active = BrightnessActiveAdjustment::SetDisplayPercent {
                display_id: display_id.clone(),
                percent,
            };
            state_tx.send_modify(|state| state.active_adjustment = Some(active));
            provider
                .set_display_percent(&display_id, percent)
                .await
                .map_err(|error| -> ServiceError { error.into() })?;
            refresh_snapshot(provider, state_tx).await?;
            state_tx.send_modify(|state| state.active_adjustment = None);
            Ok(())
        }
        BrightnessServiceCommand::AdjustDisplayPercent {
            display_id,
            delta_percent,
        } => {
            let active = BrightnessActiveAdjustment::AdjustDisplayPercent {
                display_id: display_id.clone(),
                delta_percent,
            };
            state_tx.send_modify(|state| state.active_adjustment = Some(active));
            provider
                .adjust_display_percent(&display_id, delta_percent)
                .await
                .map_err(|error| -> ServiceError { error.into() })?;
            refresh_snapshot(provider, state_tx).await?;
            state_tx.send_modify(|state| state.active_adjustment = None);
            Ok(())
        }
    }
}

async fn refresh_snapshot(
    provider: &BrightnessProvider,
    state_tx: &watch::Sender<BrightnessServiceState>,
) -> ServiceResult<()> {
    let snapshot = provider
        .snapshot()
        .await
        .map_err(|error| -> ServiceError { error.into() })?;
    state_tx.send_modify(|state| state.snapshot = snapshot);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::OpenPopoverCount;

    #[test]
    fn open_popover_count_saturates_on_close() {
        let mut count = OpenPopoverCount::default();
        count.close();
        assert_eq!(count.count, 0);

        count.open();
        count.open();
        count.close();
        count.close();
        count.close();
        assert_eq!(count.count, 0);
    }
}
