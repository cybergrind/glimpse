use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use chrono::{DateTime, Datelike, Days, Local, LocalResult, NaiveDate, TimeZone};
use futures_util::StreamExt;
use glimpse_types::{CalendarDay, CalendarEvent, CalendarMonth, CalendarMonthDay, CalendarToday};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "calendar";
const TOPICS: &[&str] = &["calendar.today"];
const METHODS: &[&str] = &["calendar.day", "calendar.month"];
const CALENDAR_SERVER_DEST: &str = "org.gnome.Shell.CalendarServer";
const CALENDAR_SERVER_PATH: &str = "/org/gnome/Shell/CalendarServer";
const CALENDAR_SERVER_IFACE: &str = "org.gnome.Shell.CalendarServer";
const SOURCES_DEST: &str = "org.gnome.evolution.dataserver.Sources5";
const SOURCES_PATH: &str = "/org/gnome/evolution/dataserver/SourceManager";
const SOURCE_IFACE: &str = "org.gnome.evolution.dataserver.Source";
const SIGNAL_UPSERT: &str = "EventsAddedOrUpdated";
const SIGNAL_REMOVE: &str = "EventsRemoved";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub uid: String,
    pub calendar_name: String,
    pub calendar_color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarServerEvent {
    pub id: String,
    pub summary: String,
    pub start_epoch: i64,
    pub end_epoch: i64,
}

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;
type CalendarServerSignalEvent = (
    String,
    String,
    i64,
    i64,
    HashMap<String, OwnedValue>,
);

#[derive(Debug, Deserialize)]
struct CalendarDayParams {
    date: String,
}

#[derive(Debug, Deserialize)]
struct CalendarMonthParams {
    month: String,
}

pub struct CalendarProvider {
    cache: CalendarToday,
    raw_events: HashMap<String, CalendarServerEvent>,
    sources: Vec<SourceInfo>,
}

impl Provider for CalendarProvider {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }

    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("calendar: starting");
            let conn = zbus::Connection::session().await?;

            self.sources = read_sources(&conn).await?;

            let proxy = zbus::Proxy::new(
                &conn,
                CALENDAR_SERVER_DEST,
                CALENDAR_SERVER_PATH,
                CALENDAR_SERVER_IFACE,
            )
            .await?;

            set_today_range(&proxy, true).await?;
            self.rebuild_cache(Local::now());
            self.emit_today(&events).await;

            let mut upserts = proxy.receive_signal(SIGNAL_UPSERT).await?;
            let mut removals = proxy.receive_signal(SIGNAL_REMOVE).await?;
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            let midnight_sleep = tokio::time::sleep(duration_until_next_midnight(Local::now()));
            tokio::pin!(midnight_sleep);

            let warmup_deadline = tokio::time::Instant::now() + Duration::from_millis(350);
            loop {
                tokio::select! {
                    _ = tokio::time::sleep_until(warmup_deadline) => break,
                    maybe_signal = upserts.next() => {
                        let Some(signal) = maybe_signal else { break };
                        let (signal_events,): (Vec<CalendarServerSignalEvent>,) = signal.body().deserialize()?;
                        for (id, summary, start_epoch, end_epoch, _meta) in signal_events {
                            self.raw_events.insert(id.clone(), CalendarServerEvent { id, summary, start_epoch, end_epoch });
                        }
                    }
                    maybe_signal = removals.next() => {
                        let Some(signal) = maybe_signal else { break };
                        let (ids,): (Vec<String>,) = signal.body().deserialize()?;
                        for id in ids {
                            self.raw_events.remove(&id);
                        }
                    }
                }
            }

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req, &conn).await;
                    }
                    maybe_signal = upserts.next() => {
                        let Some(signal) = maybe_signal else { break };
                        let (signal_events,): (Vec<CalendarServerSignalEvent>,) = signal.body().deserialize()?;
                        for (id, summary, start_epoch, end_epoch, _meta) in signal_events {
                            self.raw_events.insert(id.clone(), CalendarServerEvent { id, summary, start_epoch, end_epoch });
                        }
                        self.rebuild_cache(Local::now());
                        self.emit_today(&events).await;
                    }
                    maybe_signal = removals.next() => {
                        let Some(signal) = maybe_signal else { break };
                        let (ids,): (Vec<String>,) = signal.body().deserialize()?;
                        for id in ids {
                            self.raw_events.remove(&id);
                        }
                        self.rebuild_cache(Local::now());
                        self.emit_today(&events).await;
                    }
                    _ = tick.tick() => {
                        let now = Local::now();
                        self.rebuild_cache(now);
                        self.emit_today(&events).await;
                    }
                    _ = &mut midnight_sleep => {
                        self.sources = read_sources(&conn).await.unwrap_or_else(|e| {
                            tracing::warn!("calendar: failed to refresh source metadata: {e}");
                            self.sources.clone()
                        });
                        self.raw_events.clear();
                        if let Err(e) = set_today_range(&proxy, true).await {
                            tracing::warn!("calendar: failed to refresh time range: {e}");
                        }
                        self.rebuild_cache(Local::now());
                        self.emit_today(&events).await;
                        midnight_sleep.as_mut().reset(tokio::time::Instant::now() + duration_until_next_midnight(Local::now()));
                    }
                }
            }

            tracing::info!("calendar: stopping");
            Ok(())
        })
    }
}

impl CalendarProvider {
    fn rebuild_cache(&mut self, now: DateTime<Local>) {
        self.cache = select_visible_events(
            now,
            self.raw_events.values().cloned().collect(),
            &self.sources,
        );
    }

    async fn emit_today(&self, events: &mpsc::Sender<ProviderEvent>) {
        let _ = events
            .send(ProviderEvent {
                topic: "calendar.today".into(),
                data: serde_json::to_value(&self.cache).unwrap_or(Value::Null),
            })
            .await;
    }

    async fn handle_request(&self, req: ProviderRequest, conn: &zbus::Connection) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "calendar.today" => serde_json::to_value(&self.cache).ok(),
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call {
                method,
                params,
                reply,
            } => {
                let result = match method.as_str() {
                    "calendar.day" => fetch_calendar_day(conn, params).await,
                    "calendar.month" => fetch_calendar_month(conn, params).await,
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                let _ = reply.send(result);
            }
        }
    }
}

pub struct CalendarProviderFactory;

impl ProviderFactory for CalendarProviderFactory {
    fn name(&self) -> &'static str { NAME }
    fn topics(&self) -> &'static [&'static str] { TOPICS }
    fn methods(&self) -> &'static [&'static str] { METHODS }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(CalendarProvider {
            cache: CalendarToday {
                date: Local::now().format("%F").to_string(),
                events: Vec::new(),
            },
            raw_events: HashMap::new(),
            sources: Vec::new(),
        })
    }
}

pub fn select_visible_events(
    now: DateTime<Local>,
    events: Vec<CalendarServerEvent>,
    sources: &[SourceInfo],
) -> CalendarToday {
    let sources_by_uid: HashMap<&str, &SourceInfo> =
        sources.iter().map(|source| (source.uid.as_str(), source)).collect();

    let mut visible_events: Vec<(i64, CalendarEvent)> = events
        .into_iter()
        .filter_map(|event| {
            let start = local_from_epoch(event.start_epoch)?;
            let end = local_from_epoch(event.end_epoch)?;

            if start.date_naive() != now.date_naive() || end <= now {
                return None;
            }

            let source_uid = event.id.lines().next().unwrap_or_default();
            let source = sources_by_uid.get(source_uid);

            Some((
                event.start_epoch,
                CalendarEvent {
                    id: event.id,
                    title: event.summary,
                    start: start.to_rfc3339(),
                    end: end.to_rfc3339(),
                    location: None,
                    description: None,
                    calendar_name: source
                        .map(|source| source.calendar_name.clone())
                        .unwrap_or_default(),
                    calendar_color: source.and_then(|source| source.calendar_color.clone()),
                },
            ))
        })
        .collect();

    visible_events.sort_by_key(|(start_epoch, _)| *start_epoch);

    CalendarToday {
        date: now.format("%F").to_string(),
        events: visible_events.into_iter().map(|(_, event)| event).collect(),
    }
}

fn select_events_for_day(
    date: NaiveDate,
    now: DateTime<Local>,
    events: Vec<CalendarServerEvent>,
    sources: &[SourceInfo],
) -> CalendarDay {
    let sources_by_uid: HashMap<&str, &SourceInfo> =
        sources.iter().map(|source| (source.uid.as_str(), source)).collect();
    let start_of_day = day_start(date).expect("valid local day start");
    let end_of_day = next_day_start(date).expect("valid local next day");
    let is_today = date == now.date_naive();

    let mut selected: Vec<(i64, CalendarEvent)> = events
        .into_iter()
        .filter_map(|event| {
            let start = local_from_epoch(event.start_epoch)?;
            let end = local_from_epoch(event.end_epoch)?;

            if end <= start_of_day || start >= end_of_day {
                return None;
            }
            if is_today && end <= now {
                return None;
            }

            let source_uid = event.id.lines().next().unwrap_or_default();
            let source = sources_by_uid.get(source_uid);

            Some((
                event.start_epoch,
                CalendarEvent {
                    id: event.id,
                    title: event.summary,
                    start: start.to_rfc3339(),
                    end: end.to_rfc3339(),
                    location: None,
                    description: None,
                    calendar_name: source
                        .map(|source| source.calendar_name.clone())
                        .unwrap_or_default(),
                    calendar_color: source.and_then(|source| source.calendar_color.clone()),
                },
            ))
        })
        .collect();

    selected.sort_by_key(|(start_epoch, _)| *start_epoch);

    CalendarDay {
        date: date.format("%F").to_string(),
        events: selected.into_iter().map(|(_, event)| event).collect(),
    }
}

fn summarize_month(
    month_start_date: NaiveDate,
    events: Vec<CalendarServerEvent>,
    sources: &[SourceInfo],
) -> CalendarMonth {
    let sources_by_uid: HashMap<&str, &SourceInfo> =
        sources.iter().map(|source| (source.uid.as_str(), source)).collect();
    let next_month_date = month_start_date
        .checked_add_months(chrono::Months::new(1))
        .unwrap_or(month_start_date);
    let month_start_dt = day_start(month_start_date).expect("valid month start");
    let next_month_dt = day_start(next_month_date).expect("valid next month start");
    let last_month_day = next_month_date
        .checked_sub_days(Days::new(1))
        .unwrap_or(month_start_date);
    let mut days: HashMap<NaiveDate, Vec<String>> = HashMap::new();

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
        let Some(color) = sources_by_uid
            .get(source_uid)
            .and_then(|source| source.calendar_color.clone())
        else {
            continue;
        };

        let mut day = start.date_naive().max(month_start_date);
        let last_inclusive = if end.time() == chrono::NaiveTime::MIN {
            end.date_naive()
                .checked_sub_days(Days::new(1))
                .unwrap_or(end.date_naive())
        } else {
            end.date_naive()
        }
        .min(last_month_day);

        while day <= last_inclusive {
            let entry = days.entry(day).or_default();
            if entry.len() < 3 && !entry.iter().any(|existing| existing == &color) {
                entry.push(color.clone());
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
            date: date.format("%F").to_string(),
            colors,
        })
        .collect();
    summaries.sort_by(|a, b| a.date.cmp(&b.date));

    CalendarMonth {
        month: month_start_date.format("%Y-%m").to_string(),
        days: summaries,
    }
}

fn local_from_epoch(epoch: i64) -> Option<DateTime<Local>> {
    match Local.timestamp_opt(epoch, 0) {
        LocalResult::Single(dt) => Some(dt),
        _ => None,
    }
}

fn duration_until_next_midnight(now: DateTime<Local>) -> Duration {
    let tomorrow_start = now
        .date_naive()
        .succ_opt()
        .and_then(|date| {
            Local
                .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
                .single()
        })
        .unwrap_or(now + chrono::Duration::days(1));
    (tomorrow_start - now)
        .to_std()
        .unwrap_or_else(|_| Duration::from_secs(1))
}

async fn fetch_calendar_day(
    conn: &zbus::Connection,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let params: CalendarDayParams = serde_json::from_value(params)?;
    let date = parse_day(&params.date)?;
    let sources = read_sources(conn).await?;
    let events = fetch_range(day_start(date)?, next_day_start(date)?).await?;
    Ok(serde_json::to_value(select_events_for_day(
        date,
        Local::now(),
        events,
        &sources,
    ))?)
}

async fn fetch_calendar_month(
    conn: &zbus::Connection,
    params: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    let params: CalendarMonthParams = serde_json::from_value(params)?;
    let month = parse_month(&params.month)?;
    let sources = read_sources(conn).await?;
    let events = fetch_range(day_start(month)?, next_month_start(month)?).await?;
    Ok(serde_json::to_value(summarize_month(month, events, &sources))?)
}

async fn fetch_range(
    start: DateTime<Local>,
    end: DateTime<Local>,
) -> anyhow::Result<Vec<CalendarServerEvent>> {
    let conn = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(
        &conn,
        CALENDAR_SERVER_DEST,
        CALENDAR_SERVER_PATH,
        CALENDAR_SERVER_IFACE,
    )
    .await?;
    let mut upserts = proxy.receive_signal(SIGNAL_UPSERT).await?;
    let mut removals = proxy.receive_signal(SIGNAL_REMOVE).await?;

    proxy
        .call_method("SetTimeRange", &(start.timestamp(), end.timestamp(), true))
        .await?;

    let deadline = tokio::time::Instant::now() + Duration::from_millis(350);
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

async fn set_today_range(proxy: &zbus::Proxy<'_>, force_reload: bool) -> anyhow::Result<()> {
    let now = Local::now();
    let today_start = Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("failed to compute today start"))?;
    let tomorrow_start = today_start + chrono::Duration::days(1);

    proxy
        .call_method(
            "SetTimeRange",
            &(today_start.timestamp(), tomorrow_start.timestamp(), force_reload),
        )
        .await?;
    Ok(())
}

fn parse_day(input: &str) -> anyhow::Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(input, "%Y-%m-%d")?)
}

fn parse_month(input: &str) -> anyhow::Result<NaiveDate> {
    let month = NaiveDate::parse_from_str(&format!("{input}-01"), "%Y-%m-%d")?;
    Ok(month.with_day(1).unwrap_or(month))
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
        let calendar_color = ini_value(&data, "Calendar", "Color")
            .or_else(|| ini_value(&data, "WebDAV Backend", "Color"))
            .filter(|value| !value.is_empty());

        sources.push(SourceInfo {
            uid,
            calendar_name: display_name,
            calendar_color,
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

#[cfg(test)]
mod tests {
    use super::*;

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
            CalendarServerEvent {
                id: "source-a\nevt-4\n".into(),
                summary: "Tomorrow".into(),
                start_epoch: Local
                    .with_ymd_and_hms(2026, 4, 7, 9, 0, 0)
                    .unwrap()
                    .timestamp(),
                end_epoch: Local
                    .with_ymd_and_hms(2026, 4, 7, 9, 30, 0)
                    .unwrap()
                    .timestamp(),
            },
        ];

        let source = SourceInfo {
            uid: "source-a".into(),
            calendar_name: "Work".into(),
            calendar_color: Some("#68a3ff".into()),
        };

        let payload = select_visible_events(now, events, &[source]);

        assert_eq!(payload.date, "2026-04-06");
        assert_eq!(payload.events.len(), 2);
        assert_eq!(payload.events[0].title, "Ongoing");
        assert_eq!(payload.events[1].title, "Future");
        assert_eq!(payload.events[0].calendar_name, "Work");
        assert_eq!(payload.events[0].calendar_color.as_deref(), Some("#68a3ff"));
    }

    #[test]
    fn sorts_events_by_start_time() {
        let now = Local.with_ymd_and_hms(2026, 4, 6, 9, 0, 0).unwrap();
        let source = SourceInfo {
            uid: "source-a".into(),
            calendar_name: "Work".into(),
            calendar_color: None,
        };

        let payload = select_visible_events(
            now,
            vec![
                CalendarServerEvent {
                    id: "source-a\nevt-b\n".into(),
                    summary: "Later".into(),
                    start_epoch: Local
                        .with_ymd_and_hms(2026, 4, 6, 18, 0, 0)
                        .unwrap()
                        .timestamp(),
                    end_epoch: Local
                        .with_ymd_and_hms(2026, 4, 6, 19, 0, 0)
                        .unwrap()
                        .timestamp(),
                },
                CalendarServerEvent {
                    id: "source-a\nevt-a\n".into(),
                    summary: "Sooner".into(),
                    start_epoch: Local
                        .with_ymd_and_hms(2026, 4, 6, 10, 0, 0)
                        .unwrap()
                        .timestamp(),
                    end_epoch: Local
                        .with_ymd_and_hms(2026, 4, 6, 10, 30, 0)
                        .unwrap()
                        .timestamp(),
                },
            ],
            &[source],
        );

        assert_eq!(
            payload
                .events
                .iter()
                .map(|event| event.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Sooner", "Later"]
        );
    }

    #[test]
    fn computes_duration_until_next_midnight() {
        let now = Local.with_ymd_and_hms(2026, 4, 6, 23, 59, 45).unwrap();
        assert_eq!(duration_until_next_midnight(now), Duration::from_secs(15));
    }

    #[test]
    fn selects_all_events_for_non_today_date() {
        let now = Local.with_ymd_and_hms(2026, 4, 7, 10, 0, 0).unwrap();
        let source = SourceInfo {
            uid: "source-a".into(),
            calendar_name: "Work".into(),
            calendar_color: Some("#68a3ff".into()),
        };

        let payload = select_events_for_day(
            NaiveDate::from_ymd_opt(2026, 4, 6).unwrap(),
            now,
            vec![
                CalendarServerEvent {
                    id: "source-a\nevt-b\n".into(),
                    summary: "Morning sync".into(),
                    start_epoch: Local
                        .with_ymd_and_hms(2026, 4, 6, 9, 0, 0)
                        .unwrap()
                        .timestamp(),
                    end_epoch: Local
                        .with_ymd_and_hms(2026, 4, 6, 9, 30, 0)
                        .unwrap()
                        .timestamp(),
                },
                CalendarServerEvent {
                    id: "source-a\nevt-a\n".into(),
                    summary: "All day".into(),
                    start_epoch: Local
                        .with_ymd_and_hms(2026, 4, 6, 0, 0, 0)
                        .unwrap()
                        .timestamp(),
                    end_epoch: Local
                        .with_ymd_and_hms(2026, 4, 7, 0, 0, 0)
                        .unwrap()
                        .timestamp(),
                },
            ],
            &[source],
        );

        assert_eq!(payload.events.len(), 2);
        assert_eq!(payload.events[0].title, "All day");
        assert_eq!(payload.events[1].title, "Morning sync");
    }

    #[test]
    fn summarizes_month_with_up_to_three_unique_colors_per_day() {
        let month = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let sources = vec![
            SourceInfo {
                uid: "a".into(),
                calendar_name: "A".into(),
                calendar_color: Some("#111111".into()),
            },
            SourceInfo {
                uid: "b".into(),
                calendar_name: "B".into(),
                calendar_color: Some("#222222".into()),
            },
            SourceInfo {
                uid: "c".into(),
                calendar_name: "C".into(),
                calendar_color: Some("#333333".into()),
            },
            SourceInfo {
                uid: "d".into(),
                calendar_name: "D".into(),
                calendar_color: Some("#444444".into()),
            },
        ];

        let events = ["a", "b", "c", "d"]
            .into_iter()
            .enumerate()
            .map(|(idx, uid)| CalendarServerEvent {
                id: format!("{uid}\nevt-{idx}\n"),
                summary: format!("evt-{idx}"),
                start_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 9 + idx as u32, 0, 0)
                    .unwrap()
                    .timestamp(),
                end_epoch: Local
                    .with_ymd_and_hms(2026, 4, 6, 10 + idx as u32, 0, 0)
                    .unwrap()
                    .timestamp(),
            })
            .collect();

        let summary = summarize_month(month, events, &sources);
        let day = summary
            .days
            .iter()
            .find(|day| day.date == "2026-04-06")
            .unwrap();

        assert_eq!(day.colors, vec!["#111111", "#222222", "#333333"]);
    }
}
