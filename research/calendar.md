# Calendar Provider

**Source:** GNOME Online Accounts D-Bus, CalDAV protocol, local ICS files

**What it does:** Lists calendar events from configured accounts, provides today's schedule, upcoming events, and reminders.

## System Interface

### GNOME Online Accounts (GOA)

Service: `org.gnome.OnlineAccounts` (session bus)

Uses ObjectManager at `/org/gnome/OnlineAccounts`.

Each account has interface `org.gnome.OnlineAccounts.Account`:
- `ProviderType: String` — "google", "owncloud", "imap_smtp", "exchange", etc.
- `ProviderName: String` — human-readable
- `PresentationIdentity: String` — user email/name
- `CalendarDisabled: bool` — whether calendar is enabled for this account

Calendar-capable accounts also have `org.gnome.OnlineAccounts.Calendar`:
- `Uri: String` — CalDAV base URI

### CalDAV protocol

Standard HTTP-based calendar access:
- `PROPFIND` — discover calendars
- `REPORT` with `calendar-query` or `calendar-multiget` — fetch events in date range
- Events in iCalendar (RFC 5545) format

### Local ICS files

- `~/.local/share/gnome-calendar/` — GNOME Calendar local storage
- `~/.local/share/evolution/calendar/` — Evolution calendar data
- Any `.ics` file can be parsed directly

### iCalendar format (RFC 5545)

```
BEGIN:VCALENDAR
BEGIN:VEVENT
DTSTART:20260404T090000Z
DTEND:20260404T100000Z
SUMMARY:Team standup
DESCRIPTION:Daily standup meeting
LOCATION:Conference Room A
RRULE:FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR
STATUS:CONFIRMED
UID:unique-event-id@example.com
END:VEVENT
END:VCALENDAR
```

Key properties:
- `DTSTART` / `DTEND` — event start/end (can be date or datetime)
- `SUMMARY` — event title
- `DESCRIPTION` — event details
- `LOCATION` — event location
- `RRULE` — recurrence rule
- `STATUS` — TENTATIVE, CONFIRMED, CANCELLED
- `VALARM` — reminder/alarm

## Topics

- `calendar.events` — events in a date range
- `calendar.today` — today's events
- `calendar.upcoming` — next N upcoming events

## Methods

- `calendar.refresh()` — re-fetch events from all accounts
- `calendar.get_events(start: String, end: String) -> Vec<CalendarEvent>` — get events in ISO 8601 date range

## Types

```rust
/// Calendar event status
enum EventStatus {
    Tentative,
    Confirmed,
    Cancelled,
}

/// A calendar event
struct CalendarEvent {
    /// Unique event ID
    uid: String,
    /// Event title
    summary: String,
    description: Option<String>,
    location: Option<String>,
    /// Start time (ISO 8601)
    start: String,
    /// End time (ISO 8601)
    end: String,
    /// Whether this is an all-day event
    all_day: bool,
    status: EventStatus,
    /// Calendar/account this event belongs to
    calendar_name: String,
    /// Calendar color for UI
    calendar_color: Option<String>,
    /// Whether this is a recurring event
    is_recurring: bool,
    /// Whether there's a reminder/alarm
    has_reminder: bool,
}

/// Today's events, emitted on `calendar.today`
struct CalendarToday {
    date: String,
    events: Vec<CalendarEvent>,
}

/// Upcoming events, emitted on `calendar.upcoming`
struct CalendarUpcoming {
    events: Vec<CalendarEvent>,
}
```

## Icons

- `x-office-calendar-symbolic` — calendar
- `appointment-soon-symbolic` — upcoming event

All icons above are available in Adwaita icon theme.

## Crates

- `zbus` (5) — D-Bus client for GNOME Online Accounts
- `reqwest` — CalDAV HTTP requests (already in workspace)
- `icalendar` — iCalendar parsing crate

## Change Detection

**Timer-driven:** Re-fetch events every 5–15 minutes from CalDAV servers.

**GOA account changes:** `InterfacesAdded`/`InterfacesRemoved` on GOA ObjectManager — fires when accounts are added/removed.

**Local file changes:** inotify on local calendar directories for ICS file modifications.

## Features

- Fetch events from GNOME Online Accounts (Google, Nextcloud, etc.)
- CalDAV protocol support for any standard server
- Local ICS file support
- Today's event list
- Upcoming events view
- Recurring event expansion (RRULE parsing)
- All-day event support
- Event reminders/alarms
- Per-calendar color coding
- Multiple account/calendar support
- Date range queries
- Free/busy status
- Week number display (from clock provider)

## Notes

- CalDAV requires HTTP authentication — credentials come from GNOME Online Accounts or manual config
- Recurring event expansion (RRULE) is complex — use the `icalendar` crate's recurrence support
- GNOME Online Accounts may not be available on non-GNOME systems — support manual CalDAV config as fallback
- Event data should be cached locally to avoid slow startup
- This is a higher-complexity provider — consider implementing as a later phase
- Calendar colors are typically stored in the CalDAV CALENDAR-COLOR property
