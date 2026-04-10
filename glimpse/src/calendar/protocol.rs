use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl CalendarDate {
    pub fn from_naive_date(date: NaiveDate) -> Self {
        Self {
            year: date.year(),
            month: date.month(),
            day: date.day(),
        }
    }

    pub fn to_naive_date(self) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(self.year, self.month, self.day)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarSource {
    pub source_id: String,
    pub display_name: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarEvent {
    pub event_id: String,
    pub title: String,
    pub subtitle: String,
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub description: Option<String>,
    pub all_day: bool,
    pub source: CalendarSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarToday {
    pub date: CalendarDate,
    pub events: Vec<CalendarEvent>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarDaySnapshot {
    pub date: CalendarDate,
    pub events: Vec<CalendarEvent>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarMonthSnapshot {
    pub year: i32,
    pub month: u32,
    pub days: Vec<CalendarMonthDay>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarMonthDay {
    pub date: CalendarDate,
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

impl Default for CalendarServiceHealth {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarServiceState {
    pub health: CalendarServiceHealth,
    pub today: Option<CalendarToday>,
    pub day_cache: BTreeMap<CalendarDate, CalendarDaySnapshot>,
    pub month_cache: BTreeMap<(i32, u32), CalendarMonthSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarServiceCommand {
    LoadMonth { year: i32, month: u32 },
    LoadDay { date: CalendarDate },
    Refresh,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_state_defaults_to_empty_and_starting() {
        let state = CalendarServiceState::default();

        assert!(state.today.is_none());
        assert!(state.day_cache.is_empty());
        assert!(state.month_cache.is_empty());
        assert_eq!(state.health, CalendarServiceHealth::Starting);
    }

    #[test]
    fn calendar_today_and_day_snapshots_keep_typed_dates() {
        let date = CalendarDate {
            year: 2026,
            month: 4,
            day: 10,
        };
        let today = CalendarToday {
            date,
            events: Vec::new(),
        };
        let day = CalendarDaySnapshot {
            date,
            events: Vec::new(),
        };

        assert_eq!(today.date, date);
        assert_eq!(day.date, date);
    }
}
