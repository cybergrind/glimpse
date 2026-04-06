#[path = "../src/applets/clock/event_row.rs"]
mod event_row;

use chrono::{Local, TimeZone};

#[test]
fn formats_near_event_with_relative_time_and_duration() {
    let now = Local.with_ymd_and_hms(2026, 4, 6, 15, 15, 0).unwrap();
    let start = Local.with_ymd_and_hms(2026, 4, 6, 16, 5, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 4, 6, 16, 35, 0).unwrap();

    assert_eq!(
        event_row::format_timing_line(start, end, now),
        "in 50 min · 30 min"
    );
}

#[test]
fn formats_later_event_with_absolute_time_and_duration() {
    let now = Local.with_ymd_and_hms(2026, 4, 6, 15, 15, 0).unwrap();
    let start = Local.with_ymd_and_hms(2026, 4, 6, 17, 30, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 4, 6, 18, 15, 0).unwrap();

    assert_eq!(event_row::format_timing_line(start, end, now), "17:30 · 45 min");
}

#[test]
fn formats_ongoing_event_with_end_time() {
    let now = Local.with_ymd_and_hms(2026, 4, 6, 16, 15, 0).unwrap();
    let start = Local.with_ymd_and_hms(2026, 4, 6, 16, 5, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 4, 6, 16, 35, 0).unwrap();

    assert_eq!(event_row::format_timing_line(start, end, now), "now · ends 16:35");
}
