use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Days, Local, LocalResult, NaiveDate, NaiveTime, TimeZone};
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::calendar::protocol::{
    CalendarDate, CalendarDaySnapshot, CalendarEvent, CalendarMonthDay, CalendarMonthSnapshot,
    CalendarSource, CalendarToday,
};

const CALENDAR_SERVER_DEST: &str = "org.gnome.Shell.CalendarServer";
const CALENDAR_SERVER_PATH: &str = "/org/gnome/Shell/CalendarServer";
const CALENDAR_SERVER_IFACE: &str = "org.gnome.Shell.CalendarServer";
const SOURCES_DEST: &str = "org.gnome.evolution.dataserver.Sources5";
const SOURCES_PATH: &str = "/org/gnome/evolution/dataserver/SourceManager";
const SOURCE_IFACE: &str = "org.gnome.evolution.dataserver.Source";
const SIGNAL_UPSERT: &str = "EventsAddedOrUpdated";
const SIGNAL_REMOVE: &str = "EventsRemoved";
const RANGE_WARMUP_MS: u64 = 350;

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
type CalendarServerSignalEvent = (String, String, i64, i64, HashMap<String, OwnedValue>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceInfo {
    uid: String,
    source: CalendarSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CalendarServerEvent {
    id: String,
    summary: String,
    start_epoch: i64,
    end_epoch: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalendarChangeReason {
    EventsAddedOrUpdated,
    EventsRemoved,
}

impl fmt::Display for CalendarChangeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EventsAddedOrUpdated => "events-added-or-updated",
            Self::EventsRemoved => "events-removed",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarProviderEvent {
    Changed { reason: CalendarChangeReason },
}

#[async_trait]
pub trait CalendarBackend: Send + Sync + 'static {
    async fn load_today(&self) -> anyhow::Result<CalendarToday>;
    async fn load_day(&self, date: CalendarDate) -> anyhow::Result<CalendarDaySnapshot>;
    async fn load_month(&self, year: i32, month: u32) -> anyhow::Result<CalendarMonthSnapshot>;
    async fn set_live_range(&self, range: CalendarLiveRange) -> anyhow::Result<()>;
    async fn listen(
        &self,
        events: mpsc::Sender<CalendarProviderEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()>;
}

#[derive(Clone)]
pub struct CalendarProvider {
    fetch_session: zbus::Connection,
    listen_session: zbus::Connection,
    live_range_tx: tokio::sync::watch::Sender<CalendarLiveRange>,
}

impl CalendarProvider {
    pub async fn new(session: zbus::Connection) -> anyhow::Result<Self> {
        let listen_session = zbus::Connection::session().await?;
        let _ = calendar_server_proxy(&session).await?;
        let _ = calendar_server_proxy(&listen_session).await?;
        let today = Local::now().date_naive();
        let (live_range_tx, _) = tokio::sync::watch::channel(CalendarLiveRange {
            start: day_start(today)?,
            end: next_day_start(today)?,
        });
        Ok(Self {
            fetch_session: session,
            listen_session,
            live_range_tx,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarLiveRange {
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
}

#[async_trait]
impl CalendarBackend for CalendarProvider {
    async fn load_today(&self) -> anyhow::Result<CalendarToday> {
        let proxy = calendar_server_proxy(&self.fetch_session).await?;
        let sources = read_sources(&self.fetch_session).await?;
        let now = Local::now();
        let today = now.date_naive();
        let events =
            collect_events_for_range(&proxy, day_start(today)?, next_day_start(today)?, true)
                .await?;
        Ok(select_visible_events(now, events, &sources))
    }

    async fn load_day(&self, date: CalendarDate) -> anyhow::Result<CalendarDaySnapshot> {
        let date = date
            .to_naive_date()
            .ok_or_else(|| anyhow::anyhow!("invalid calendar date"))?;
        let sources = read_sources(&self.fetch_session).await?;
        let events =
            fetch_range(&self.fetch_session, day_start(date)?, next_day_start(date)?).await?;
        Ok(select_events_for_day(date, Local::now(), events, &sources))
    }

    async fn load_month(&self, year: i32, month: u32) -> anyhow::Result<CalendarMonthSnapshot> {
        let month = NaiveDate::from_ymd_opt(year, month, 1)
            .ok_or_else(|| anyhow::anyhow!("invalid calendar month {year}-{month}"))?;
        let sources = read_sources(&self.fetch_session).await?;
        let events = fetch_range(
            &self.fetch_session,
            day_start(month)?,
            next_month_start(month)?,
        )
        .await?;
        Ok(summarize_month(month, events, &sources))
    }

    async fn set_live_range(&self, range: CalendarLiveRange) -> anyhow::Result<()> {
        let _ = self.live_range_tx.send(range);
        Ok(())
    }

    async fn listen(
        &self,
        events: mpsc::Sender<CalendarProviderEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let proxy = calendar_server_proxy(&self.listen_session).await?;
        let mut upserts = proxy.receive_signal(SIGNAL_UPSERT).await?;
        let mut removals = proxy.receive_signal(SIGNAL_REMOVE).await?;
        let mut live_range = self.live_range_tx.subscribe();
        let initial_range = live_range.borrow().clone();
        set_time_range(&proxy, initial_range.start, initial_range.end, false).await?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                range_change = live_range.changed() => {
                    if range_change.is_err() {
                        return Ok(());
                    }
                    let range = live_range.borrow_and_update().clone();
                    set_time_range(&proxy, range.start, range.end, false).await?;
                }
                maybe_signal = upserts.next() => {
                    let Some(_) = maybe_signal else {
                        return Ok(());
                    };
                    if events.send(CalendarProviderEvent::Changed {
                        reason: CalendarChangeReason::EventsAddedOrUpdated,
                    }).await.is_err() {
                        return Ok(());
                    }
                }
                maybe_signal = removals.next() => {
                    let Some(_) = maybe_signal else {
                        return Ok(());
                    };
                    if events.send(CalendarProviderEvent::Changed {
                        reason: CalendarChangeReason::EventsRemoved,
                    }).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn map_event(
    event: CalendarServerEvent,
    source: Option<&SourceInfo>,
) -> Option<(i64, CalendarEvent)> {
    let start = local_from_epoch(event.start_epoch)?;
    let end = local_from_epoch(event.end_epoch)?;

    Some((
        event.start_epoch,
        CalendarEvent {
            event_id: event.id,
            title: event.summary,
            subtitle: String::new(),
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            location: None,
            description: None,
            all_day: is_all_day_event(start, end),
            source: source
                .map(|source| source.source.clone())
                .unwrap_or_default(),
        },
    ))
}

fn select_visible_events(
    now: DateTime<Local>,
    events: Vec<CalendarServerEvent>,
    sources: &[SourceInfo],
) -> CalendarToday {
    let sources_by_uid: HashMap<&str, &SourceInfo> = sources
        .iter()
        .map(|source| (source.uid.as_str(), source))
        .collect();

    let mut visible_events: Vec<(i64, CalendarEvent)> = events
        .into_iter()
        .filter_map(|event| {
            let start = local_from_epoch(event.start_epoch)?;
            let end = local_from_epoch(event.end_epoch)?;

            if start.date_naive() != now.date_naive() || end <= now {
                return None;
            }

            let source_uid = event.id.lines().next().unwrap_or_default().to_owned();
            map_event(event, sources_by_uid.get(source_uid.as_str()).copied())
        })
        .collect();

    visible_events.sort_by_key(|(start_epoch, _)| *start_epoch);

    CalendarToday {
        date: CalendarDate::from_naive_date(now.date_naive()),
        events: visible_events.into_iter().map(|(_, event)| event).collect(),
    }
}

fn select_events_for_day(
    date: NaiveDate,
    _now: DateTime<Local>,
    events: Vec<CalendarServerEvent>,
    sources: &[SourceInfo],
) -> CalendarDaySnapshot {
    let sources_by_uid: HashMap<&str, &SourceInfo> = sources
        .iter()
        .map(|source| (source.uid.as_str(), source))
        .collect();
    let start_of_day = day_start(date).expect("valid local day start");
    let end_of_day = next_day_start(date).expect("valid local next day");

    let mut selected: Vec<(i64, CalendarEvent)> = events
        .into_iter()
        .filter_map(|event| {
            let start = local_from_epoch(event.start_epoch)?;
            let end = local_from_epoch(event.end_epoch)?;

            if end <= start_of_day || start >= end_of_day {
                return None;
            }

            let source_uid = event.id.lines().next().unwrap_or_default().to_owned();
            map_event(event, sources_by_uid.get(source_uid.as_str()).copied())
        })
        .collect();

    selected.sort_by_key(|(start_epoch, _)| *start_epoch);

    CalendarDaySnapshot {
        date: CalendarDate::from_naive_date(date),
        events: selected.into_iter().map(|(_, event)| event).collect(),
    }
}

fn summarize_month(
    month_start_date: NaiveDate,
    events: Vec<CalendarServerEvent>,
    sources: &[SourceInfo],
) -> CalendarMonthSnapshot {
    let sources_by_uid: HashMap<&str, &SourceInfo> = sources
        .iter()
        .map(|source| (source.uid.as_str(), source))
        .collect();
    let next_month_date = month_start_date
        .checked_add_months(chrono::Months::new(1))
        .unwrap_or(month_start_date);
    let month_start_dt = day_start(month_start_date).expect("valid month start");
    let next_month_dt = day_start(next_month_date).expect("valid next month start");
    let last_month_day = next_month_date
        .checked_sub_days(Days::new(1))
        .unwrap_or(month_start_date);
    let mut days: HashMap<NaiveDate, Vec<String>> = HashMap::new();
    let mut event_days: BTreeMap<NaiveDate, Vec<(i64, CalendarEvent)>> = BTreeMap::new();

    for event in events {
        let Some(start) = local_from_epoch(event.start_epoch) else {
            continue;
        };
        let Some(end) = local_from_epoch(event.end_epoch) else {
            continue;
        };
        if end <= month_start_dt || start >= next_month_dt {
            continue;
        }

        let source_uid = event.id.lines().next().unwrap_or_default();
        let color = sources_by_uid
            .get(source_uid)
            .and_then(|source| source.source.color.clone());
        let mapped = map_event(event.clone(), sources_by_uid.get(source_uid).copied());

        let mut day = start.date_naive().max(month_start_date);
        let last_inclusive = if end.time() == NaiveTime::MIN {
            end.date_naive()
                .checked_sub_days(Days::new(1))
                .unwrap_or(end.date_naive())
        } else {
            end.date_naive()
        }
        .min(last_month_day);

        while day <= last_inclusive {
            let entry = days.entry(day).or_default();
            if let Some(color) = color.as_ref() {
                if entry.len() < 3 && !entry.iter().any(|existing| existing == color) {
                    entry.push(color.clone());
                }
            }
            if let Some((start_epoch, mapped)) = mapped.clone() {
                event_days
                    .entry(day)
                    .or_default()
                    .push((start_epoch, mapped));
            }
            let Some(next) = day.checked_add_days(Days::new(1)) else {
                break;
            };
            day = next;
        }
    }

    let mut summaries: Vec<_> = days
        .into_iter()
        .map(|(date, colors)| CalendarMonthDay {
            date: CalendarDate::from_naive_date(date),
            colors,
        })
        .collect();
    summaries.sort_by_key(|day| day.date);
    let day_snapshots = event_days
        .into_iter()
        .map(|(date, mut entries)| {
            entries.sort_by_key(|(start_epoch, _)| *start_epoch);
            entries.dedup_by(|(_, left), (_, right)| left.event_id == right.event_id);
            (
                CalendarDate::from_naive_date(date),
                CalendarDaySnapshot {
                    date: CalendarDate::from_naive_date(date),
                    events: entries.into_iter().map(|(_, event)| event).collect(),
                },
            )
        })
        .collect();

    CalendarMonthSnapshot {
        year: month_start_date.year(),
        month: month_start_date.month(),
        days: summaries,
        day_snapshots,
    }
}

fn local_from_epoch(epoch: i64) -> Option<DateTime<Local>> {
    match Local.timestamp_opt(epoch, 0) {
        LocalResult::Single(dt) => Some(dt),
        _ => None,
    }
}

async fn calendar_server_proxy<'a>(conn: &'a zbus::Connection) -> anyhow::Result<zbus::Proxy<'a>> {
    Ok(zbus::Proxy::new(
        conn,
        CALENDAR_SERVER_DEST,
        CALENDAR_SERVER_PATH,
        CALENDAR_SERVER_IFACE,
    )
    .await?)
}

async fn fetch_range(
    conn: &zbus::Connection,
    start: DateTime<Local>,
    end: DateTime<Local>,
) -> anyhow::Result<Vec<CalendarServerEvent>> {
    let proxy = calendar_server_proxy(conn).await?;
    collect_events_for_range(&proxy, start, end, true).await
}

async fn collect_events_for_range(
    proxy: &zbus::Proxy<'_>,
    start: DateTime<Local>,
    end: DateTime<Local>,
    force_reload: bool,
) -> anyhow::Result<Vec<CalendarServerEvent>> {
    let mut upserts = proxy.receive_signal(SIGNAL_UPSERT).await?;
    let mut removals = proxy.receive_signal(SIGNAL_REMOVE).await?;
    set_time_range(proxy, start, end, force_reload).await?;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(RANGE_WARMUP_MS);
    let mut events = HashMap::new();

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            maybe_signal = upserts.next() => {
                let Some(signal) = maybe_signal else { break };
                let (signal_events,): (Vec<CalendarServerSignalEvent>,) = signal.body().deserialize()?;
                for (id, summary, start_epoch, end_epoch, _meta) in signal_events {
                    events.insert(id.clone(), CalendarServerEvent { id, summary, start_epoch, end_epoch });
                }
            }
            maybe_signal = removals.next() => {
                let Some(signal) = maybe_signal else { break };
                let (ids,): (Vec<String>,) = signal.body().deserialize()?;
                for id in ids {
                    events.remove(&id);
                }
            }
        }
    }

    Ok(events.into_values().collect())
}

async fn set_time_range(
    proxy: &zbus::Proxy<'_>,
    start: DateTime<Local>,
    end: DateTime<Local>,
    force_reload: bool,
) -> anyhow::Result<()> {
    proxy
        .call_method(
            "SetTimeRange",
            &(start.timestamp(), end.timestamp(), force_reload),
        )
        .await?;
    Ok(())
}

fn day_start(date: NaiveDate) -> anyhow::Result<DateTime<Local>> {
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("failed to compute local start for {}", date))
}

fn next_day_start(date: NaiveDate) -> anyhow::Result<DateTime<Local>> {
    let next = date
        .checked_add_days(Days::new(1))
        .ok_or_else(|| anyhow::anyhow!("date overflow"))?;
    day_start(next)
}

fn next_month_start(date: NaiveDate) -> anyhow::Result<DateTime<Local>> {
    let next = date
        .checked_add_months(chrono::Months::new(1))
        .ok_or_else(|| anyhow::anyhow!("month overflow"))?;
    day_start(next.with_day(1).unwrap_or(next))
}

async fn read_sources(conn: &zbus::Connection) -> anyhow::Result<Vec<SourceInfo>> {
    let proxy = zbus::Proxy::new(
        conn,
        SOURCES_DEST,
        SOURCES_PATH,
        "org.freedesktop.DBus.ObjectManager",
    )
    .await?;

    let managed: ManagedObjects = proxy.call("GetManagedObjects", &()).await?;
    let mut sources = Vec::new();

    for (_path, interfaces) in managed {
        let Some(source_iface) = interfaces.get(SOURCE_IFACE) else {
            continue;
        };
        let Some(data) = source_iface.get("Data").and_then(value_to_string) else {
            continue;
        };
        if !data.contains("\n[Calendar]\n") && !data.contains("[Calendar]\n") {
            continue;
        }

        let Some(uid) = source_iface.get("UID").and_then(value_to_string) else {
            continue;
        };

        let display_name = ini_value(&data, "Data Source", "DisplayName").unwrap_or_default();
        let color = ini_value(&data, "Calendar", "Color")
            .or_else(|| ini_value(&data, "WebDAV Backend", "Color"))
            .filter(|value| !value.is_empty());

        sources.push(SourceInfo {
            uid: uid.clone(),
            source: CalendarSource {
                source_id: uid,
                display_name,
                color,
            },
        });
    }

    Ok(sources)
}

fn value_to_string(value: &OwnedValue) -> Option<String> {
    value.try_clone().ok()?.try_into().ok()
}

fn ini_value(data: &str, section: &str, key: &str) -> Option<String> {
    let mut in_section = false;
    for line in data.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_section = &line[1..line.len() - 1] == section;
            continue;
        }
        if !in_section {
            continue;
        }
        let (candidate_key, candidate_value) = line.split_once('=')?;
        if candidate_key == key {
            return Some(candidate_value.to_string());
        }
    }
    None
}

fn is_all_day_event(start: DateTime<Local>, end: DateTime<Local>) -> bool {
    start.time() == NaiveTime::MIN && end.time() == NaiveTime::MIN && (end - start).num_days() >= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_mapping_preserves_source_metadata() {
        let event = CalendarServerEvent {
            id: "source-a\nevt-1\n".into(),
            summary: "Design sync".into(),
            start_epoch: Local
                .with_ymd_and_hms(2026, 4, 6, 16, 5, 0)
                .unwrap()
                .timestamp(),
            end_epoch: Local
                .with_ymd_and_hms(2026, 4, 6, 16, 35, 0)
                .unwrap()
                .timestamp(),
        };
        let source = SourceInfo {
            uid: "source-a".into(),
            source: CalendarSource {
                source_id: "source-a".into(),
                display_name: "Work".into(),
                color: Some("#68a3ff".into()),
            },
        };

        let (_, mapped) = map_event(event, Some(&source)).expect("mapped event");

        assert_eq!(mapped.source.display_name, "Work");
        assert_eq!(mapped.source.color.as_deref(), Some("#68a3ff"));
        assert_eq!(mapped.title, "Design sync");
    }

    #[test]
    fn keeps_ongoing_and_future_events_for_today_only() {
        let now = Local.with_ymd_and_hms(2026, 4, 6, 15, 15, 0).unwrap();
        let events = vec![
            CalendarServerEvent {
                id: "source-a\nevt-1\n".into(),
                summary: "Ongoing".into(),
                start_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 15, 0, 0)
                    .unwrap()
                    .timestamp(),
                end_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 15, 30, 0)
                    .unwrap()
                    .timestamp(),
            },
            CalendarServerEvent {
                id: "source-a\nevt-2\n".into(),
                summary: "Future".into(),
                start_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 17, 30, 0)
                    .unwrap()
                    .timestamp(),
                end_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 18, 15, 0)
                    .unwrap()
                    .timestamp(),
            },
            CalendarServerEvent {
                id: "source-a\nevt-3\n".into(),
                summary: "Ended".into(),
                start_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 14, 0, 0)
                    .unwrap()
                    .timestamp(),
                end_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 14, 45, 0)
                    .unwrap()
                    .timestamp(),
            },
        ];

        let source = SourceInfo {
            uid: "source-a".into(),
            source: CalendarSource {
                source_id: "source-a".into(),
                display_name: "Work".into(),
                color: Some("#68a3ff".into()),
            },
        };

        let payload = select_visible_events(now, events, &[source]);

        assert_eq!(
            payload.date,
            CalendarDate {
                year: 2026,
                month: 4,
                day: 6,
            }
        );
        assert_eq!(payload.events.len(), 2);
        assert_eq!(payload.events[0].title, "Ongoing");
        assert_eq!(payload.events[1].title, "Future");
        assert_eq!(payload.events[0].source.display_name, "Work");
        assert_eq!(payload.events[0].source.color.as_deref(), Some("#68a3ff"));
    }

    #[test]
    fn selected_day_snapshot_keeps_ended_events_for_today() {
        let now = Local.with_ymd_and_hms(2026, 4, 6, 15, 15, 0).unwrap();
        let date = now.date_naive();
        let events = vec![
            CalendarServerEvent {
                id: "source-a\nevt-1\n".into(),
                summary: "Ended".into(),
                start_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 14, 0, 0)
                    .unwrap()
                    .timestamp(),
                end_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 14, 45, 0)
                    .unwrap()
                    .timestamp(),
            },
            CalendarServerEvent {
                id: "source-a\nevt-2\n".into(),
                summary: "Future".into(),
                start_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 17, 30, 0)
                    .unwrap()
                    .timestamp(),
                end_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 18, 15, 0)
                    .unwrap()
                    .timestamp(),
            },
        ];
        let source = SourceInfo {
            uid: "source-a".into(),
            source: CalendarSource {
                source_id: "source-a".into(),
                display_name: "Work".into(),
                color: Some("#68a3ff".into()),
            },
        };

        let payload = select_events_for_day(date, now, events, &[source]);

        assert_eq!(payload.events.len(), 2);
        assert_eq!(payload.events[0].title, "Ended");
        assert_eq!(payload.events[1].title, "Future");
    }

    #[test]
    fn month_summary_keeps_day_snapshots_for_instant_day_switching() {
        let month_start = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let event = CalendarServerEvent {
            id: "source-a\nevt-1\n".into(),
            summary: "Design sync".into(),
            start_epoch: Local
                .with_ymd_and_hms(2026, 4, 10, 16, 5, 0)
                .unwrap()
                .timestamp(),
            end_epoch: Local
                .with_ymd_and_hms(2026, 4, 10, 16, 35, 0)
                .unwrap()
                .timestamp(),
        };
        let source = SourceInfo {
            uid: "source-a".into(),
            source: CalendarSource {
                source_id: "source-a".into(),
                display_name: "Work".into(),
                color: Some("#68a3ff".into()),
            },
        };

        let summary = summarize_month(month_start, vec![event], &[source]);
        let day = summary
            .day_snapshots
            .get(&CalendarDate {
                year: 2026,
                month: 4,
                day: 10,
            })
            .expect("day snapshot should be present");

        assert_eq!(day.events.len(), 1);
        assert_eq!(day.events[0].title, "Design sync");
    }
}
