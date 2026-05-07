# Sunset

Sunset warms your screen at night. It can follow sunrise and sunset for your location, or use fixed clock times.

The default schedule is off, so enabling the service will not change your colors until you configure it.

## Automatic Sunset

Use this when you want Glimpse to follow daylight where you live:

```toml
[location]
provider = "static"
latitude = 52.2297
longitude = 21.0122

[night_light]
enabled = true
schedule = "automatic"
temperature = 4200
transition_minutes = 15
```

Then enable it:

```sh
systemctl --user enable --now glimpse-sunset.service
```

## Fixed Schedule

Use this when you want the same hours every day:

```toml
[night_light]
enabled = true
schedule = "schedule"
start_time = "20:30"
end_time = "07:00"
temperature = 4200
transition_minutes = 15
```

## Choosing Temperature

| Temperature | Feel |
|---|---|
| `6500` | Normal daylight. |
| `5000` | Slightly warm. |
| `4200` | Comfortable evening warmth. |
| `3500` | Very warm, good late at night. |

Start with `4200`. Lower it if your screen still feels harsh at night.

## Location Choices

| Provider | Best for |
|---|---|
| `static` | Most desktops and privacy-friendly setups. |
| `geoclue` | Laptops where desktop location services are already working. |
| `ipapi` | Approximate location from network lookup. |

For a desktop setup, static coordinates are the least surprising choice.

## Troubleshooting

| Problem | What to check |
|---|---|
| Nothing changes | Make sure `schedule` is not `off`. |
| Sunset happens at the wrong time | Check latitude and longitude. |
| It works in one compositor but not another | Check the service log with `journalctl --user -u glimpse-sunset.service -e`. |
