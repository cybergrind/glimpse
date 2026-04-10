use std::{error::Error, sync::Arc, time::Duration};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    calendar::protocol::{
        CalendarDate, CalendarDaySnapshot, CalendarMonthSnapshot, CalendarServiceCommand,
        CalendarServiceHealth, CalendarServiceState,
    },
    providers::calendar::{CalendarBackend, CalendarChangeReason, CalendarProvider, CalendarProviderEvent},
};

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

#[derive(Clone)]
pub struct CalendarServiceHandle {
    commands: mpsc::Sender<CalendarServiceCommand>,
    state: watch::Receiver<CalendarServiceState>,
}

impl CalendarServiceHandle {
    pub fn new(session: zbus::Connection) -> Self {
        let (state_tx, state) = watch::channel(CalendarServiceState::default());
        let (commands, cmd_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            run_calendar_service(session, state_tx, cmd_rx).await;
        });

        Self { commands, state }
    }

    #[cfg(test)]
    fn from_backend(backend: Arc<dyn CalendarBackend>) -> Self {
        let (state_tx, state) = watch::channel(CalendarServiceState::default());
        let (commands, cmd_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            let mut cmd_rx = cmd_rx;
            let _ = run_connected(backend, state_tx, &mut cmd_rx).await;
        });

        Self { commands, state }
    }

    pub fn subscribe(&self) -> watch::Receiver<CalendarServiceState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: CalendarServiceCommand,
    ) -> Result<(), mpsc::error::SendError<CalendarServiceCommand>> {
        self.commands.send(command).await
    }
}

async fn run_calendar_service(
    session: zbus::Connection,
    state_tx: watch::Sender<CalendarServiceState>,
    cmd_rx: mpsc::Receiver<CalendarServiceCommand>,
) {
    let mut attempt = 0u32;
    let mut cmd_rx = cmd_rx;

    loop {
        attempt += 1;
        let _ = state_tx.send_modify(|state| {
            state.health = if attempt == 1 {
                CalendarServiceHealth::Starting
            } else {
                CalendarServiceHealth::Reconnecting { attempt }
            };
        });

        let provider = match CalendarProvider::new(session.clone()).await {
            Ok(provider) => Arc::new(provider) as Arc<dyn CalendarBackend>,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "calendar service: failed to start provider");
                let _ = state_tx.send_modify(|state| {
                    state.health = CalendarServiceHealth::Degraded {
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
                tracing::warn!(error = %error, attempt, "calendar service: worker failed");
                let _ = state_tx.send_modify(|state| {
                    state.health = CalendarServiceHealth::Degraded {
                        message: error.to_string(),
                    };
                });
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn run_connected(
    provider: Arc<dyn CalendarBackend>,
    state_tx: watch::Sender<CalendarServiceState>,
    cmd_rx: &mut mpsc::Receiver<CalendarServiceCommand>,
) -> ServiceResult<()> {
    let cancel = CancellationToken::new();
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut listener = tokio::spawn({
        let provider = provider.clone();
        let cancel = cancel.clone();
        async move { provider.listen(event_tx, cancel).await }
    });

    refresh_all(&*provider, &state_tx).await?;
    let _ = state_tx.send_modify(|state| state.health = CalendarServiceHealth::Ready);

    let result = loop {
        tokio::select! {
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(CalendarProviderEvent::Changed { reason }) => {
                        log_provider_change(reason);
                        if let Err(error) = refresh_all(&*provider, &state_tx).await {
                            tracing::warn!(error = %error, "calendar service: refresh failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = CalendarServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        } else {
                            let _ = state_tx.send_modify(|state| state.health = CalendarServiceHealth::Ready);
                        }
                    }
                    None => break Err(service_error("calendar provider event channel closed")),
                }
            }
            maybe_command = cmd_rx.recv() => {
                match maybe_command {
                    Some(command) => {
                        if let Err(error) = handle_command(&*provider, &state_tx, command).await {
                            tracing::warn!(error = %error, "calendar service: command failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = CalendarServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        } else {
                            let _ = state_tx.send_modify(|state| state.health = CalendarServiceHealth::Ready);
                        }
                    }
                    None => break Ok(()),
                }
            }
            join = &mut listener => {
                break match join {
                    Ok(Ok(())) => Err(service_error("calendar listener exited")),
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(service_error(format!("calendar listener task failed: {error}"))),
                };
            }
        }
    };

    cancel.cancel();
    result
}

fn log_provider_change(reason: CalendarChangeReason) {
    tracing::debug!(reason = %reason, "calendar service: provider changed");
}

async fn handle_command(
    provider: &dyn CalendarBackend,
    state_tx: &watch::Sender<CalendarServiceState>,
    command: CalendarServiceCommand,
) -> anyhow::Result<()> {
    match command {
        CalendarServiceCommand::LoadMonth { year, month } => {
            let snapshot = provider.load_month(year, month).await?;
            let _ = state_tx.send_modify(|state| {
                state.month_cache.insert((year, month), snapshot);
            });
        }
        CalendarServiceCommand::LoadDay { date } => {
            let snapshot = provider.load_day(date).await?;
            let _ = state_tx.send_modify(|state| {
                state.day_cache.insert(date, snapshot);
            });
        }
        CalendarServiceCommand::Refresh => {
            refresh_all(provider, state_tx).await?;
        }
    }

    Ok(())
}

async fn refresh_all(
    provider: &dyn CalendarBackend,
    state_tx: &watch::Sender<CalendarServiceState>,
) -> anyhow::Result<()> {
    let today = provider.load_today().await?;
    let (day_keys, month_keys) = {
        let state = state_tx.borrow();
        (
            state.day_cache.keys().copied().collect::<Vec<_>>(),
            state.month_cache.keys().copied().collect::<Vec<_>>(),
        )
    };

    let mut refreshed_days: Vec<(CalendarDate, CalendarDaySnapshot)> = Vec::new();
    for day in day_keys {
        refreshed_days.push((day, provider.load_day(day).await?));
    }

    let mut refreshed_months: Vec<((i32, u32), CalendarMonthSnapshot)> = Vec::new();
    for (year, month) in month_keys {
        refreshed_months.push(((year, month), provider.load_month(year, month).await?));
    }

    let _ = state_tx.send_modify(|state| {
        state.today = Some(today);
        for (date, day) in refreshed_days {
            state.day_cache.insert(date, day);
        }
        for (key, month) in refreshed_months {
            state.month_cache.insert(key, month);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::{
        calendar::protocol::{CalendarMonthDay, CalendarToday},
        providers::calendar::CalendarProviderEvent,
    };

    #[derive(Default)]
    struct MockState {
        loaded_months: Vec<(i32, u32)>,
    }

    #[derive(Default)]
    struct MockBackend {
        state: Mutex<MockState>,
    }

    #[async_trait]
    impl CalendarBackend for MockBackend {
        async fn load_today(&self) -> anyhow::Result<CalendarToday> {
            Ok(CalendarToday {
                date: CalendarDate {
                    year: 2026,
                    month: 4,
                    day: 10,
                },
                events: Vec::new(),
            })
        }

        async fn load_day(&self, date: CalendarDate) -> anyhow::Result<CalendarDaySnapshot> {
            Ok(CalendarDaySnapshot {
                date,
                events: Vec::new(),
            })
        }

        async fn load_month(
            &self,
            year: i32,
            month: u32,
        ) -> anyhow::Result<CalendarMonthSnapshot> {
            self.state
                .lock()
                .expect("mock state poisoned")
                .loaded_months
                .push((year, month));
            Ok(CalendarMonthSnapshot {
                year,
                month,
                days: vec![CalendarMonthDay {
                    date: CalendarDate {
                        year,
                        month,
                        day: 10,
                    },
                    colors: vec!["#68a3ff".into()],
                }],
            })
        }

        async fn listen(
            &self,
            _events: mpsc::Sender<CalendarProviderEvent>,
            cancel: CancellationToken,
        ) -> anyhow::Result<()> {
            cancel.cancelled().await;
            Ok(())
        }
    }

    #[tokio::test]
    async fn load_month_command_publishes_month_snapshot() {
        let handle = CalendarServiceHandle::from_backend(Arc::new(MockBackend::default()));
        let mut state = handle.subscribe();

        state.changed().await.expect("initial state publication");
        let _ = state.borrow_and_update();

        handle
            .send(CalendarServiceCommand::LoadMonth {
                year: 2026,
                month: 4,
            })
            .await
            .expect("load month command should be accepted");

        state.changed().await.expect("load month should publish state");
        assert!(state.borrow().month_cache.contains_key(&(2026, 4)));
    }
}
