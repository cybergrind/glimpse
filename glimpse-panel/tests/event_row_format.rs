#[path = "../src/applets/clock/event_row.rs"]
mod event_row;

use chrono::{Local, TimeZone};

#[test]
fn formats_near_event_with_relative_time_and_duration() {
    let now = Local.with_ymd_and_hms(2026, 4, 6, 15, 15, 0).unwrap();
    let start = Local.with_ymd_and_hms(2026, 4, 6, 16, 5, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 4, 6, 16, 35, 0).unwrap();

    assert_eq!(
        event_row::format_timing_line(start, end, now.date_naive(), now),
        "in 50 min · 30 min"
    );
}

#[test]
fn formats_later_event_with_absolute_time_and_duration() {
    let now = Local.with_ymd_and_hms(2026, 4, 6, 15, 15, 0).unwrap();
    let start = Local.with_ymd_and_hms(2026, 4, 6, 17, 30, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 4, 6, 18, 15, 0).unwrap();

    assert_eq!(
        event_row::format_timing_line(start, end, now.date_naive(), now),
        "17:30 · 45 min"
    );
}

#[test]
fn formats_ongoing_event_with_end_time() {
    let now = Local.with_ymd_and_hms(2026, 4, 6, 16, 15, 0).unwrap();
    let start = Local.with_ymd_and_hms(2026, 4, 6, 16, 5, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 4, 6, 16, 35, 0).unwrap();

    assert_eq!(
        event_row::format_timing_line(start, end, now.date_naive(), now),
        "now · ends 16:35"
    );
}

#[test]
fn formats_midnight_spanning_event_as_all_day() {
    let now = Local.with_ymd_and_hms(2026, 4, 6, 15, 15, 0).unwrap();
    let start = Local.with_ymd_and_hms(2026, 4, 6, 0, 0, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 4, 7, 0, 0, 0).unwrap();

    assert_eq!(
        event_row::format_timing_line(start, end, now.date_naive(), now),
        "All day"
    );
}

#[test]
fn formats_non_today_event_with_absolute_time() {
    let now = Local.with_ymd_and_hms(2026, 4, 7, 15, 15, 0).unwrap();
    let selected_date = Local.with_ymd_and_hms(2026, 4, 8, 0, 0, 0).unwrap().date_naive();
    let start = Local.with_ymd_and_hms(2026, 4, 8, 17, 30, 0).unwrap();
    let end = Local.with_ymd_and_hms(2026, 4, 8, 18, 15, 0).unwrap();

    assert_eq!(
        event_row::format_timing_line(start, end, selected_date, now),
        "17:30 · 45 min"
    );
}
