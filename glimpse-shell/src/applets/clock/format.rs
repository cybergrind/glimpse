use chrono::{DateTime, Local, NaiveDate};

use glimpse_core::services::{
    calendar_events::{CalendarEvent, MonthKey},
    clock::State as ClockState,
};

pub const DEFAULT_LABEL_FORMAT: &str = "%a %-d %b, %H:%M";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "%A, %-d %B %Y";

pub fn label(format: &str, state: &ClockState) -> String {
    state.now.format(format).to_string()
}

pub fn tooltip(format: &str, state: &ClockState) -> String {
    state.now.format(format).to_string()
}

pub fn selected_weekday(date: NaiveDate) -> String {
    date.format("%A").to_string()
}

pub fn selected_date(date: NaiveDate) -> String {
    date.format("%-d %b, %Y").to_string()
}

pub fn month_label(month: MonthKey) -> String {
    month
        .to_naive_date()
        .map(|date| date.format("%B %Y").to_string())
        .unwrap_or_default()
}

pub fn event_time(event: &CalendarEvent, selected_date: NaiveDate, now: DateTime<Local>) -> String {
    let Some((start, end)) = parse_event_times(event) else {
        return String::new();
    };

    if event.all_day {
        return "All day".into();
    }

    let duration = duration_label(start, end);
    if selected_date != now.date_naive() {
        return format!("{} · {}", start.format("%H:%M"), duration);
    }

    if now >= start && now < end {
        return format!("now · ends {}", end.format("%H:%M"));
    }

    let until_start = start - now;
    if until_start.num_minutes() < 60 && until_start.num_minutes() >= 0 {
        format!("in {} min · {}", until_start.num_minutes(), duration)
    } else {
        format!("{} · {}", start.format("%H:%M"), duration)
    }
}

fn parse_event_times(event: &CalendarEvent) -> Option<(DateTime<Local>, DateTime<Local>)> {
    let start = DateTime::parse_from_rfc3339(&event.start).ok()?;
    let end = DateTime::parse_from_rfc3339(&event.end).ok()?;
    Some((start.with_timezone(&Local), end.with_timezone(&Local)))
}

fn duration_label(start: DateTime<Local>, end: DateTime<Local>) -> String {
    let minutes = (end - start).num_minutes().max(0);
    if minutes < 60 {
        return format!("{minutes} min");
    }

    let hours = minutes / 60;
    let rest = minutes % 60;
    if rest == 0 {
        format!("{hours} h")
    } else {
        format!("{hours} h {rest} min")
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn duration_label_uses_hours_for_long_events() {
        let start = Local.with_ymd_and_hms(2026, 4, 30, 10, 0, 0).unwrap();
        let end = Local.with_ymd_and_hms(2026, 4, 30, 11, 30, 0).unwrap();

        assert_eq!(duration_label(start, end), "1 h 30 min");
    }
}
