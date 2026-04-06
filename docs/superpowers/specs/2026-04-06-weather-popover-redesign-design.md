# Weather Popover Redesign

**Date:** 2026-04-06

**Goal:** Redesign the weather popover so it stays calm and compact, but feels more expressive and useful than the current implementation.

## Approved Layout

```text
┌──────────────────────────────────────────────┐
│  ☁  12°                              Warsaw, PL │
│     Overcast · Feels like 9°                  │
├──────────────────────────────────────────────┤
│  Next 5 hours                                 │
│                                               │
│  13:00    14:00    15:00    16:00    17:00    │
│  ☁        🌧       🌧       ☁        ☁        │
│  12°      11°      11°      10°      9°       │
├──────────────────────────────────────────────┤
│  Details                                      │
│                                               │
│  High           14°      Low             8°  │
│  Humidity       82%      Wind      18 km/h NW│
│  Rain         1.2 mm     Pressure    1008 hPa│
│  UV index        1       Sun       06:12/19:48│
├──────────────────────────────────────────────┤
│  Forecast                                     │
│                                               │
│  Tue   🌧  Rain · 3mm                  7° / 11°│
│  Wed   ⛅  Cloudy                       6° / 13°│
│  Thu   ☀  Clear                        5° / 15°│
│  Fri   ☁  Overcast                     9° / 16°│
│  Sat   ☁  Overcast · 1mm              10° / 17°│
└──────────────────────────────────────────────┘
```

## Design Decisions

- Keep the hero section, but make it compact so it feels consistent with other applets.
- Hero uses two rows:
  - row 1: icon, current temperature, location
  - row 2: condition and feels-like
- Remove `High` and `Low` from the hero. Move them into the details section.
- Replace the fixed 4-hour strip with a configurable future strip that defaults to 5 hours.
- Keep details in rows rather than tiles, but increase contrast and grouping so the section feels less flat.
- Keep the forecast section configurable, but restore a richer row format closer to the earlier design.

## Visual Hierarchy

- Hero is still first, but not oversized.
- The future hourly strip is the primary supporting section.
- Details are useful but should feel more intentional than the current neutral rows.
- The forecast section is last, but should regain some richness and scanability.

The redesign should rely on spacing, alignment, and typography rather than large decorative elements or dense boxed cards.

## Section Behavior

### Hero

- One main weather icon
- One large current temperature
- Location aligned to the opposite side of the first row
- Second row contains `Condition · Feels like X°`
- No `High/Low` here

### Next Hours

- Controlled by config field `hourly_slots`
- Default `5`
- `0` disables the section entirely
- Render future slots only
- Start at the first future hour, not the current hour
- Each slot includes:
  - time
  - icon
  - temperature
- Use even spacing and homogeneous columns
- Clamp to a small upper bound so the strip stays readable in the popover

### Details

- Present as key/value rows in two columns
- Keep 8 fields:
  - High
  - Low
  - Humidity
  - Wind
  - Rain
  - Pressure
  - UV index
  - Sunrise/Sunset
- Avoid the old tile-heavy look
- Improve hierarchy with stronger value emphasis and clearer pair grouping
- Keep the section compact enough for a popover, not a dashboard

### Forecast

- Controlled by config field `forecast_days`
- `0` disables the section entirely
- `1..=10` renders that many forecast rows
- Values above `10` clamp to `10`
- The first rendered row is tomorrow, not today

- Keep list rows
- Each row includes:
  - day
  - icon
  - condition
  - precipitation hint when present
  - low/high pair
- Keep rows compact and readable
- Temperatures stay aligned at the far right as `low / high`

## Implementation Constraints

- Add a weather config field `hourly_slots` with default `5`.
- Keep `forecast_days` with default `5`.
- Preserve the current data model unless the new layout requires a small formatting helper.
- Prefer layout and spacing changes over adding more widgets than necessary.
- Follow existing weather applet patterns and CSS organization.
- Keep the hero visually aligned with the rest of the panel popover ecosystem.

## Testing Scope

- Verify the hourly strip renders future slots only.
- Verify `hourly_slots = 0` hides the hourly section.
- Verify `hourly_slots = 5` renders 5 hourly slots.
- Verify the hero no longer renders `High/Low`.
- Verify details includes `High/Low`.
- Verify details renders 8 items.
- Verify `forecast_days = 0` hides the forecast section.
- Verify `forecast_days = 5` renders 5 forecast rows starting from tomorrow.
- Verify values above `10` clamp to `10`.
