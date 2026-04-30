use std::{
    collections::{BTreeSet, VecDeque},
    time::Duration,
};

use chrono::{Datelike, Local, Months};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Instant, sleep},
};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

use super::{
    model::{CalendarMonthSnapshot, Command, Health, MonthKey, State},
    provider::{CalendarClient, CalendarProviderEvent, LiveRange},
};

const COMMAND_QUEUE_SIZE: usize = 16;
const PROVIDER_EVENT_QUEUE_SIZE: usize = 16;
const REFRESH_INTERVAL: Duration = Duration::from_secs(600);

pub type CalendarEventsHandle = ServiceHandle<State, Command>;

pub struct CalendarEventsService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    session: zbus::Connection,
}

#[derive(Debug)]
struct MonthLoad {
    result: anyhow::Result<CalendarMonthSnapshot>,
}

struct Listener {
    events: mpsc::Receiver<CalendarProviderEvent>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl CalendarEventsService {
    pub fn new(session: zbus::Connection) -> (Self, CalendarEventsHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                state_tx,
                command_rx,
                session,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        let mut startup_preload = None;
        let client = loop {
            match CalendarClient::new(self.session.clone()).await {
                Ok(client) => break client,
                Err(error) => {
                    self.publish_health(Health::Degraded(error.to_string()));
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        command = self.command_rx.recv() => {
                            match command {
                                Some(ServiceCommand::Command(Command::PreloadAround(month))) => {
                                    startup_preload = Some(month);
                                }
                                Some(ServiceCommand::Control(Control::Shutdown)) | None => return,
                                Some(ServiceCommand::Command(Command::Refresh))
                                | Some(ServiceCommand::Control(Control::Start(_)))
                                | Some(ServiceCommand::Control(Control::Reconfigure(_))) => {}
                            }
                        }
                        _ = sleep(Duration::from_secs(5)) => {}
                    }
                }
            }
        };

        let (live_range_tx, live_range_rx) = watch::channel(None);
        let mut listener = Some(start_listener(&client, live_range_rx.clone()));
        let listener_retry = sleep(Duration::MAX);
        tokio::pin!(listener_retry);
        let mut listener_retry_scheduled = false;

        let mut pending = VecDeque::new();
        let mut queued = BTreeSet::new();
        let mut inflight: Option<(MonthKey, JoinHandle<MonthLoad>)> = None;
        let mut active_months = BTreeSet::new();
        let refresh = sleep(REFRESH_INTERVAL);
        tokio::pin!(refresh);

        self.set_preload_window(
            startup_preload.unwrap_or_else(|| MonthKey::from_date(Local::now().date_naive())),
            &mut active_months,
            &mut pending,
            &mut queued,
        );
        self.sync_live_range(&live_range_tx, &active_months);
        start_next_load(
            &client,
            &mut pending,
            &mut queued,
            &mut inflight,
            &self.state_tx,
        );

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(Command::PreloadAround(month))) => {
                        self.set_preload_window(month, &mut active_months, &mut pending, &mut queued);
                        abort_inactive_load(&active_months, &mut inflight);
                        self.sync_live_range(&live_range_tx, &active_months);
                        start_next_load(&client, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                    }
                    Some(ServiceCommand::Command(Command::Refresh)) => {
                        self.queue_active_months(&active_months, &mut pending, &mut queued);
                        start_next_load(&client, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => break,
                    Some(ServiceCommand::Control(Control::Start(_)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(_))) => {}
                },
                event = async {
                    let Some(listener) = listener.as_mut() else {
                        return None;
                    };
                    listener.events.recv().await
                }, if listener.is_some() => {
                    match event {
                        Some(CalendarProviderEvent::Changed(reason)) => {
                            tracing::debug!(?reason, "calendar provider changed");
                            self.queue_active_months(&active_months, &mut pending, &mut queued);
                            start_next_load(&client, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                        }
                        None => {
                            self.publish_health(Health::Degraded("calendar events listener stopped".into()));
                            stop_listener(listener.take());
                            listener_retry.as_mut().reset(Instant::now() + Duration::from_secs(5));
                            listener_retry_scheduled = true;
                        }
                    }
                }
                _ = &mut refresh => {
                    self.queue_active_months(&active_months, &mut pending, &mut queued);
                    start_next_load(&client, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                    refresh.as_mut().reset(Instant::now() + REFRESH_INTERVAL);
                }
                _ = &mut listener_retry, if listener_retry_scheduled => {
                    listener = Some(start_listener(&client, live_range_rx.clone()));
                    listener_retry_scheduled = false;
                }
                loaded = async {
                    let Some((_, task)) = inflight.as_mut() else {
                        return None;
                    };
                    Some(task.await)
                }, if inflight.is_some() => {
                    let loaded_key = inflight.take().map(|(key, _)| key);
                    if let Some(key) = loaded_key {
                        self.state_tx.send_if_modified(|state| state.loading_months.remove(&key));
                    }
                    if let Some(key) = loaded_key.filter(|key| active_months.contains(key)) {
                        match loaded {
                            Some(Ok(MonthLoad { result: Ok(month), .. })) => {
                                self.publish_month(key, month);
                            }
                            Some(Ok(MonthLoad { result: Err(error), .. })) => {
                                tracing::warn!(%error, ?key, "failed to load calendar month");
                                self.publish_health(Health::Degraded(error.to_string()));
                            }
                            Some(Err(error)) => {
                                tracing::warn!(%error, "calendar month loader task failed");
                                self.publish_health(Health::Degraded(format!("calendar loader task failed: {error}")));
                            }
                            None => {}
                        }
                    }
                    self.sync_live_range(&live_range_tx, &active_months);
                    start_next_load(&client, &mut pending, &mut queued, &mut inflight, &self.state_tx);
                }
            }
        }

        if let Some((_, task)) = inflight {
            task.abort();
        }
        stop_listener(listener);
    }

    fn set_preload_window(
        &self,
        month: MonthKey,
        active_months: &mut BTreeSet<MonthKey>,
        pending: &mut VecDeque<MonthKey>,
        queued: &mut BTreeSet<MonthKey>,
    ) {
        active_months.clear();
        active_months.extend(preload_window(month));
        pending.retain(|key| active_months.contains(key));
        queued.retain(|key| active_months.contains(key));
        self.evict_inactive_months(active_months);

        for key in active_months.iter().copied() {
            queue_month(&self.state_tx, pending, queued, key, false);
        }
    }

    fn queue_active_months(
        &self,
        active_months: &BTreeSet<MonthKey>,
        pending: &mut VecDeque<MonthKey>,
        queued: &mut BTreeSet<MonthKey>,
    ) {
        for key in active_months.iter().copied() {
            queue_month(&self.state_tx, pending, queued, key, true);
        }
    }

    fn sync_live_range(
        &self,
        live_range_tx: &watch::Sender<Option<LiveRange>>,
        active_months: &BTreeSet<MonthKey>,
    ) {
        let Some(range) = live_range_for_months(active_months.iter().copied().collect()) else {
            return;
        };
        let _ = live_range_tx.send(Some(range));
    }

    fn evict_inactive_months(&self, active_months: &BTreeSet<MonthKey>) {
        self.state_tx.send_if_modified(|state| {
            let before_months = state.month_cache.len();
            let before_loading = state.loading_months.len();
            state
                .month_cache
                .retain(|key, _| active_months.contains(key));
            state
                .loading_months
                .retain(|key| active_months.contains(key));
            state.month_cache.len() != before_months || state.loading_months.len() != before_loading
        });
    }

    fn publish_health(&self, health: Health) {
        self.state_tx.send_if_modified(|state| {
            if state.health == health {
                false
            } else {
                state.health = health;
                true
            }
        });
    }

    fn publish_month(&self, key: MonthKey, month: CalendarMonthSnapshot) {
        self.state_tx.send_if_modified(|state| {
            state.health = Health::Ready;
            state.month_cache.insert(key, month);
            true
        });
    }
}

fn queue_month(
    state_tx: &watch::Sender<State>,
    pending: &mut VecDeque<MonthKey>,
    queued: &mut BTreeSet<MonthKey>,
    key: MonthKey,
    force: bool,
) {
    let state = state_tx.borrow();
    if state.loading_months.contains(&key)
        || (!force && state.month_cache.contains_key(&key))
        || !queued.insert(key)
    {
        return;
    }
    drop(state);
    pending.push_back(key);
}

fn start_next_load(
    client: &CalendarClient,
    pending: &mut VecDeque<MonthKey>,
    queued: &mut BTreeSet<MonthKey>,
    inflight: &mut Option<(MonthKey, JoinHandle<MonthLoad>)>,
    state_tx: &watch::Sender<State>,
) {
    if inflight.is_some() {
        return;
    }
    let Some(key) = pending.pop_front() else {
        return;
    };
    queued.remove(&key);
    state_tx.send_if_modified(|state| {
        state.health = Health::Loading;
        state.loading_months.insert(key)
    });

    let client = client.clone();
    *inflight = Some((
        key,
        tokio::spawn(async move {
            let result = client.load_month(key).await;
            MonthLoad { result }
        }),
    ));
}

fn abort_inactive_load(
    active_months: &BTreeSet<MonthKey>,
    inflight: &mut Option<(MonthKey, JoinHandle<MonthLoad>)>,
) {
    if inflight
        .as_ref()
        .is_some_and(|(key, _)| !active_months.contains(key))
    {
        if let Some((_, task)) = inflight.take() {
            task.abort();
        }
    }
}

fn start_listener(
    client: &CalendarClient,
    live_range: watch::Receiver<Option<LiveRange>>,
) -> Listener {
    let (events, event_rx) = mpsc::channel(PROVIDER_EVENT_QUEUE_SIZE);
    let cancel = CancellationToken::new();
    let task = tokio::spawn({
        let client = client.clone();
        let cancel = cancel.clone();
        async move {
            if let Err(error) = client.listen(events, live_range, cancel).await {
                tracing::warn!(%error, "calendar events listener failed");
            }
        }
    });

    Listener {
        events: event_rx,
        cancel,
        task,
    }
}

fn stop_listener(listener: Option<Listener>) {
    if let Some(listener) = listener {
        listener.cancel.cancel();
        listener.task.abort();
    }
}

fn preload_window(month: MonthKey) -> BTreeSet<MonthKey> {
    let mut months = BTreeSet::from([month]);
    if let Some(next) = month.next() {
        months.insert(next);
    }
    months
}

fn live_range_for_months(months: Vec<MonthKey>) -> Option<LiveRange> {
    let min = months.iter().min()?.to_naive_date()?;
    let max = months.iter().max()?.to_naive_date()?;
    let end = max.checked_add_months(Months::new(1))?;
    Some(LiveRange {
        start: local_day_start(min)?,
        end: local_day_start(end)?,
    })
}

fn local_day_start(date: chrono::NaiveDate) -> Option<chrono::DateTime<Local>> {
    use chrono::TimeZone;
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_range_spans_loaded_months() {
        let range = live_range_for_months(vec![
            MonthKey {
                year: 2026,
                month: 5,
            },
            MonthKey {
                year: 2026,
                month: 4,
            },
        ])
        .unwrap();

        assert_eq!(
            range.start.date_naive(),
            chrono::NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()
        );
        assert_eq!(
            range.end.date_naive(),
            chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()
        );
    }

    #[test]
    fn preload_window_keeps_visible_month_and_next_only() {
        assert_eq!(
            preload_window(MonthKey {
                year: 2026,
                month: 12,
            }),
            BTreeSet::from([
                MonthKey {
                    year: 2026,
                    month: 12,
                },
                MonthKey {
                    year: 2027,
                    month: 1,
                },
            ])
        );
    }
}
