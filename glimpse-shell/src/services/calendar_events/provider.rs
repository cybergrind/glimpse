use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};

use chrono::{DateTime, Datelike, Days, Local, LocalResult, NaiveDate, NaiveTime, TimeZone};
use futures_util::StreamExt;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use zbus::zvariant::OwnedValue;

use crate::dbus::calendar::{
    CalendarServerEventPayload, CalendarServerProxy, EvolutionSourceManagerProxy,
    SIGNAL_EVENTS_ADDED_OR_UPDATED, SIGNAL_EVENTS_REMOVED, SOURCE_IFACE,
};

use super::model::{
    CalendarDate, CalendarDaySnapshot, CalendarEvent, CalendarMonthDay, CalendarMonthSnapshot,
    CalendarSource, MonthKey,
};

const RANGE_FIRST_SIGNAL_TIMEOUT: Duration = Duration::from_secs(2);
const RANGE_QUIET_WINDOW: Duration = Duration::from_millis(120);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarProviderEvent {
    Changed(CalendarChangeReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveRange {
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
}

#[derive(Clone)]
pub struct CalendarClient {
    fetch_session: zbus::Connection,
    listen_session: zbus::Connection,
}

impl CalendarClient {
    pub async fn new(session: zbus::Connection) -> anyhow::Result<Self> {
        let listen_session = zbus::Connection::session().await?;
        let _ = CalendarServerProxy::new(&session).await?;
        let _ = CalendarServerProxy::new(&listen_session).await?;
        Ok(Self {
            fetch_session: session,
            listen_session,
        })
    }

    pub async fn load_month(&self, key: MonthKey) -> anyhow::Result<CalendarMonthSnapshot> {
        let month = key
            .to_naive_date()
            .ok_or_else(|| anyhow::anyhow!("invalid calendar month"))?;
        let sources = read_sources(&self.fetch_session).await?;
        let events = fetch_range(
            &self.fetch_session,
            day_start(month)?,
            next_month_start(month)?,
        )
        .await?;

        Ok(summarize_month(month, events, &sources))
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<CalendarProviderEvent>,
        mut live_range: watch::Receiver<Option<LiveRange>>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let proxy = CalendarServerProxy::new(&self.listen_session).await?;
        let dynamic_proxy = calendar_server_dynamic_proxy(&self.listen_session).await?;
        let mut upserts = dynamic_proxy
            .receive_signal(SIGNAL_EVENTS_ADDED_OR_UPDATED)
            .await?;
        let mut removals = dynamic_proxy.receive_signal(SIGNAL_EVENTS_REMOVED).await?;

        let initial_range = { live_range.borrow().clone() };
        if let Some(range) = initial_range {
            proxy
                .set_time_range(range.start.timestamp(), range.end.timestamp(), false)
                .await?;
        }

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                changed = live_range.changed() => {
                    if changed.is_err() {
                        return Ok(());
                    }
                    let Some(range) = live_range.borrow_and_update().clone() else {
                        continue;
                    };
                    proxy.set_time_range(range.start.timestamp(), range.end.timestamp(), false).await?;
                }
                maybe_signal = upserts.next() => {
                    if maybe_signal.is_none() {
                        return Ok(());
                    }
                    if events.send(CalendarProviderEvent::Changed(CalendarChangeReason::EventsAddedOrUpdated)).await.is_err() {
                        return Ok(());
                    }
                }
                maybe_signal = removals.next() => {
                    if maybe_signal.is_none() {
                        return Ok(());
                    }
                    if events.send(CalendarProviderEvent::Changed(CalendarChangeReason::EventsRemoved)).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

async fn calendar_server_dynamic_proxy<'a>(
    conn: &'a zbus::Connection,
) -> anyhow::Result<zbus::Proxy<'a>> {
    Ok(zbus::Proxy::new(
        conn,
        crate::dbus::calendar::CALENDAR_SERVER_DEST,
        crate::dbus::calendar::CALENDAR_SERVER_PATH,
        crate::dbus::calendar::CALENDAR_SERVER_IFACE,
    )
    .await?)
}

async fn fetch_range(
    conn: &zbus::Connection,
    start: DateTime<Local>,
    end: DateTime<Local>,
) -> anyhow::Result<Vec<CalendarServerEvent>> {
    let proxy = CalendarServerProxy::new(conn).await?;
    let dynamic_proxy = calendar_server_dynamic_proxy(conn).await?;
    let mut upserts = dynamic_proxy
        .receive_signal(SIGNAL_EVENTS_ADDED_OR_UPDATED)
        .await?;
    let mut removals = dynamic_proxy.receive_signal(SIGNAL_EVENTS_REMOVED).await?;
    proxy
        .set_time_range(start.timestamp(), end.timestamp(), true)
        .await?;

    let deadline = tokio::time::Instant::now() + RANGE_FIRST_SIGNAL_TIMEOUT;
    let quiet = tokio::time::sleep_until(deadline);
    tokio::pin!(quiet);
    let mut events = HashMap::new();
    let mut received_signal = false;

    loop {
        tokio::select! {
            _ = &mut quiet => {
                if !received_signal {
                    tracing::debug!(
                        start = %start,
                        end = %end,
                        "calendar range produced no events before timeout"
                    );
                }
                break;
            }
            maybe_signal = upserts.next() => {
                let Some(signal) = maybe_signal else { break };
                received_signal = true;
                let (signal_events,): (Vec<CalendarServerEventPayload>,) = signal.body().deserialize()?;
                for (id, summary, start_epoch, end_epoch, _meta) in signal_events {
                    events.insert(id.clone(), CalendarServerEvent { id, summary, start_epoch, end_epoch });
                }
                quiet.as_mut().reset(tokio::time::Instant::now() + RANGE_QUIET_WINDOW);
            }
            maybe_signal = removals.next() => {
                let Some(signal) = maybe_signal else { break };
                received_signal = true;
                let (ids,): (Vec<String>,) = signal.body().deserialize()?;
                for id in ids {
                    events.remove(&id);
                }
                quiet.as_mut().reset(tokio::time::Instant::now() + RANGE_QUIET_WINDOW);
            }
        }
    }

    Ok(events.into_values().collect())
}

async fn read_sources(conn: &zbus::Connection) -> anyhow::Result<Vec<SourceInfo>> {
    let proxy = EvolutionSourceManagerProxy::new(conn).await?;
    let managed = proxy.get_managed_objects().await?;
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
            .and_then(|source| source.source.color.clone())
            .unwrap_or_default();
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
            if entry.len() < 3 && !entry.iter().any(|existing| existing == &color) {
                entry.push(color.clone());
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
        key: MonthKey::from_date(month_start_date),
        days: summaries,
        day_snapshots,
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
            start: start.to_rfc3339(),
            end: end.to_rfc3339(),
            location: None,
            all_day: is_all_day_event(start, end),
            source: source
                .map(|source| source.source.clone())
                .unwrap_or_default(),
        },
    ))
}

fn local_from_epoch(epoch: i64) -> Option<DateTime<Local>> {
    match Local.timestamp_opt(epoch, 0) {
        LocalResult::Single(dt) => Some(dt),
        _ => None,
    }
}

fn day_start(date: NaiveDate) -> anyhow::Result<DateTime<Local>> {
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
        .ok_or_else(|| anyhow::anyhow!("failed to compute local start for {date}"))
}

fn next_month_start(date: NaiveDate) -> anyhow::Result<DateTime<Local>> {
    let next = date
        .checked_add_months(chrono::Months::new(1))
        .ok_or_else(|| anyhow::anyhow!("month overflow"))?;
    day_start(next.with_day(1).unwrap_or(next))
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
    fn ini_value_reads_requested_section_key() {
        let data = "[Data Source]\nDisplayName=Work\n[Calendar]\nColor=#ff0000\n";

        assert_eq!(
            ini_value(data, "Data Source", "DisplayName"),
            Some("Work".into())
        );
        assert_eq!(ini_value(data, "Calendar", "Color"), Some("#ff0000".into()));
        assert_eq!(ini_value(data, "Calendar", "Missing"), None);
    }
}
