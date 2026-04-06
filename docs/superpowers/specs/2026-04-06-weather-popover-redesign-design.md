# Weather Popover Redesign

**Date:** 2026-04-06

**Goal:** Redesign the weather popover to feel calmer and more spacious while preserving the existing feature set and the hero section.

## Approved Layout

```text
┌──────────────────────────────────────────────┐
│  ☁  12°                              Warsaw, PL │
│     Overcast · Feels like 9°                  │
├──────────────────────────────────────────────┤
│  Next 4 hours                                 │
│                                               │
│  13:00      14:00      15:00      16:00       │
│  ☁          🌧         🌧         ☁           │
│  12°        11°        11°        10°         │
├──────────────────────────────────────────────┤
│  Details                                      │
│                                               │
│  High            14°     Humidity      82%   │
│  Low              8°     Wind        18 km/h │
│  Rain          1.2 mm    Pressure   1008 hPa │
│  UV index          1                          │
├──────────────────────────────────────────────┤
│  Forecast                                     │
│                                               │
│  Today   ☁   Overcast              8° / 14°  │
│  Tue     🌧  Rain                  7° / 11°  │
│  Wed     ⛅  Cloudy                6° / 13°  │
│  Thu     ☀   Clear                 5° / 15°  │
│  Fri     ☁   Overcast              9° / 16°  │
└──────────────────────────────────────────────┘
```

## Design Decisions

- Keep the hero section, but make it compact so it feels consistent with other applets.
- Hero uses two rows:
  - row 1: icon, current temperature, location
  - row 2: condition and feels-like
- Remove `High` and `Low` from the hero. Move them into the details section.
- Keep the 4-hour forecast, but remove the `Now` cell. The hero already covers current conditions, so the strip should start at `+1h`.
- Keep details as a simple facts section instead of equal-weight stat tiles.
- Keep the forecast section, but make its row count configurable and treat it as secondary content with quieter visual emphasis than the hero and 4-hour strip.

## Visual Hierarchy

- Hero is still first, but not oversized.
- The 4-hour forecast is the primary supporting section.
- Details are useful but visually restrained.
- The forecast section is last and lighter in emphasis.

The redesign should rely on spacing, alignment, and typography rather than large decorative elements or dense boxed cards.

## Section Behavior

### Hero

- One main weather icon
- One large current temperature
- Location aligned to the opposite side of the first row
- Second row contains `Condition · Feels like X°`
- No `High/Low` here

### Next 4 Hours

- Exactly 4 future slots
- Start at the first future hour, not the current hour
- Each slot includes:
  - time
  - icon
  - temperature
- Use even spacing and homogeneous columns

### Details

- Present as key/value rows in two columns
- Prioritized fields:
  - High
  - Low
  - Humidity
  - Wind
  - Rain
  - Pressure
  - UV index
  - Sunrise/Sunset
- Avoid the current tile-heavy look

### Forecast

- Controlled by config field `forecast_days`
- `0` disables the section entirely
- `1..=10` renders that many forecast rows
- Values above `10` clamp to `10`

- Keep list rows
- Each row includes:
  - day
  - icon
  - condition
  - low/high pair
- Keep rows compact and readable

## Implementation Constraints

- Add a weather config field `forecast_days` with default `5`.
- Preserve the current data model unless the new layout requires a small formatting helper.
- Prefer layout and spacing changes over adding more widgets than necessary.
- Follow existing weather applet patterns and CSS organization.
- Keep the hero visually aligned with the rest of the panel popover ecosystem.

## Testing Scope

- Verify the 4-hour section renders future slots only.
- Verify the hero no longer renders `High/Low`.
- Verify details includes `High/Low`.
- Verify details renders 8 items.
- Verify `forecast_days = 0` hides the forecast section.
- Verify `forecast_days = 5` renders 5 forecast rows.
- Verify values above `10` clamp to `10`.
