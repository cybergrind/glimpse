use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, Months, NaiveDate};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonthKey {
    pub year: i32,
    pub month: u32,
}

impl MonthKey {
    pub fn from_date(date: NaiveDate) -> Self {
        Self {
            year: date.year(),
            month: date.month(),
        }
    }

    pub fn to_naive_date(self) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(self.year, self.month, 1)
    }

    pub fn next(self) -> Option<Self> {
        let date = self.to_naive_date()?.checked_add_months(Months::new(1))?;
        Some(Self::from_date(date))
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
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub all_day: bool,
    pub source: CalendarSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarDaySnapshot {
    pub date: CalendarDate,
    pub events: Vec<CalendarEvent>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarMonthSnapshot {
    pub key: MonthKey,
    pub days: Vec<CalendarMonthDay>,
    pub day_snapshots: BTreeMap<CalendarDate, CalendarDaySnapshot>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CalendarMonthDay {
    pub date: CalendarDate,
    pub colors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Starting,
    Ready,
    Loading,
    Degraded(String),
}

impl Default for Health {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub health: Health,
    pub month_cache: BTreeMap<MonthKey, CalendarMonthSnapshot>,
    pub loading_months: BTreeSet<MonthKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    PreloadAround(MonthKey),
    #[allow(dead_code)]
    Refresh,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_key_computes_next_across_year_boundary() {
        assert_eq!(
            MonthKey {
                year: 2026,
                month: 12
            }
            .next(),
            Some(MonthKey {
                year: 2027,
                month: 1
            })
        );
    }
}
