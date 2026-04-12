use std::{error::Error, sync::Arc, time::Duration};

use chrono::{Datelike, Days, Local, Months, NaiveDate, TimeZone};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    calendar::protocol::{
        CalendarDate, CalendarDaySnapshot, CalendarMonthSnapshot, CalendarServiceCommand,
        CalendarServiceHealth, CalendarServiceState,
    },
    providers::calendar::{
        CalendarBackend, CalendarChangeReason, CalendarLiveRange, CalendarProvider,
        CalendarProviderEvent,
    },
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
    sync_live_range(&*provider, &state_tx).await?;
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
                            if let Err(error) = sync_live_range(&*provider, &state_tx).await {
                                tracing::warn!(error = %error, "calendar service: live range sync failed");
                            }
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
                            if let Err(error) = sync_live_range(&*provider, &state_tx).await {
                                tracing::warn!(error = %error, "calendar service: live range sync failed");
                            }
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
                state.month_cache.clear();
                state.day_cache.retain(|date, _| {
                    (date.year == year && date.month == month) || is_today(*date)
                });
                for (date, day) in snapshot.day_snapshots.clone() {
                    state.day_cache.insert(date, day);
                }
                state.month_cache.insert((year, month), snapshot);
            });
        }
        CalendarServiceCommand::LoadDay { date } => {
            let snapshot = provider.load_day(date).await?;
            let _ = state_tx.send_modify(|state| {
                let cached_months = state
                    .month_cache
                    .keys()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>();
                state.day_cache.retain(|cached, _| {
                    cached_months.contains(&(cached.year, cached.month))
                        || *cached == date
                        || is_today(*cached)
                });
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
    let covered_months = month_keys
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();

    let mut refreshed_days: Vec<(CalendarDate, CalendarDaySnapshot)> = Vec::new();
    for day in day_keys
        .into_iter()
        .filter(|day| !covered_months.contains(&(day.year, day.month)))
    {
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
            for (date, day) in month.day_snapshots.clone() {
                state.day_cache.insert(date, day);
            }
            state.month_cache.insert(key, month);
        }
    });
    Ok(())
}

async fn sync_live_range(
    provider: &dyn CalendarBackend,
    state_tx: &watch::Sender<CalendarServiceState>,
) -> anyhow::Result<()> {
    let range = {
        let state = state_tx.borrow();
        compute_live_range(&state)
    };
    provider.set_live_range(range).await
}

fn compute_live_range(state: &CalendarServiceState) -> CalendarLiveRange {
    let today = Local::now().date_naive();
    let mut start = day_start_from_date(today);
    let mut end = next_day_start_from_date(today);

    for date in state
        .day_cache
        .keys()
        .filter_map(|date| date.to_naive_date())
    {
        start = start.min(day_start_from_date(date));
        end = end.max(next_day_start_from_date(date));
    }

    for (year, month) in state.month_cache.keys() {
        if let Some(month_start) = NaiveDate::from_ymd_opt(*year, *month, 1) {
            start = start.min(day_start_from_date(month_start));
            if let Some(next_month) = month_start.checked_add_months(Months::new(1)) {
                end = end.max(day_start_from_date(next_month));
            }
        }
    }

    CalendarLiveRange { start, end }
}

fn day_start_from_date(date: NaiveDate) -> chrono::DateTime<Local> {
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
        .expect("valid local day start")
}

fn next_day_start_from_date(date: NaiveDate) -> chrono::DateTime<Local> {
    let next = date.checked_add_days(Days::new(1)).expect("date overflow");
    day_start_from_date(next)
}

fn is_today(date: CalendarDate) -> bool {
    Some(Local::now().date_naive()) == date.to_naive_date()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::{
        calendar::protocol::{CalendarMonthDay, CalendarToday},
        providers::calendar::{CalendarLiveRange, CalendarProviderEvent},
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

        async fn load_month(&self, year: i32, month: u32) -> anyhow::Result<CalendarMonthSnapshot> {
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
                day_snapshots: std::iter::once((
                    CalendarDate {
                        year,
                        month,
                        day: 10,
                    },
                    CalendarDaySnapshot {
                        date: CalendarDate {
                            year,
                            month,
                            day: 10,
                        },
                        events: vec![],
                    },
                ))
                .collect(),
            })
        }

        async fn set_live_range(&self, _range: CalendarLiveRange) -> anyhow::Result<()> {
            Ok(())
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

        state
            .changed()
            .await
            .expect("load month should publish state");
        assert!(state.borrow().month_cache.contains_key(&(2026, 4)));
    }

    #[tokio::test]
    async fn load_month_command_prepopulates_day_cache_for_loaded_month() {
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

        state
            .changed()
            .await
            .expect("load month should publish state");
        assert!(state.borrow().day_cache.contains_key(&CalendarDate {
            year: 2026,
            month: 4,
            day: 10,
        }));
    }

    #[tokio::test]
    async fn load_month_command_replaces_old_month_cache() {
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
            .expect("first load month command should be accepted");
        state
            .changed()
            .await
            .expect("first month should publish state");
        let _ = state.borrow_and_update();

        handle
            .send(CalendarServiceCommand::LoadMonth {
                year: 2026,
                month: 5,
            })
            .await
            .expect("second load month command should be accepted");
        state
            .changed()
            .await
            .expect("second month should publish state");

        let state = state.borrow();
        assert_eq!(state.month_cache.len(), 1);
        assert!(state.month_cache.contains_key(&(2026, 5)));
        assert!(!state.month_cache.contains_key(&(2026, 4)));
    }
}
