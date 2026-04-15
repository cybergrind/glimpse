# Theme Auto-Switch Design

## Goal

Add a first-class theme domain that can drive the desktop color preference from Glimpse config.

Config should support:

- `theme.mode = "light"`
- `theme.mode = "dark"`
- `theme.mode = "auto"`

`auto` uses shared solar times to decide whether the effective theme should be light or dark.

The feature targets the desktop-wide color preference used by GTK/libadwaita and portal-following apps. It does not attempt to restyle arbitrary applications that ignore the standard dark-style preference.

## Scope

In scope:

- config support for `light`, `dark`, `auto`
- a shared `theme` provider/service domain
- desktop preference read/apply support
- root-app wiring so Glimpse follows the effective theme it is driving
- reuse of the shared solar provider for `auto`

Out of scope:

- automatic theme file switching via `theme.name`
- per-app overrides
- a UI for editing theme mode
- supporting non-standard app theming systems

## Architecture

Add a new shared domain in `glimpse`:

- `glimpse/src/theme/mod.rs`
- `glimpse/src/theme/protocol.rs`
- `glimpse/src/theme/provider.rs`
- `glimpse/src/theme/service.rs`

Responsibilities:

- `protocol.rs`
  - shared config-facing and runtime-facing types
- `provider.rs`
  - desktop integration for reading and applying the effective light/dark preference
- `service.rs`
  - auto-mode policy, solar-time evaluation, state reduction, retries, and scheduling
- `mod.rs`
  - public exports

The root app does not decide theme policy. It constructs the service, sends config updates, and subscribes to stable state.

## Config Contract

`ThemeConfig` remains the config entry point.

- `theme.name: Option<String>`
  - selected Glimpse stylesheet
  - unchanged
- `theme.mode: ThemeMode`
  - requested theme mode

`ThemeMode` becomes:

- `Light`
- `Dark`
- `Auto`

This replaces the current `System` mode. The service becomes the source of the effective desktop preference instead of passing through the system unchanged.

`theme.name` and `theme.mode` stay independent:

- `theme.name` selects the stylesheet
- `theme.mode` selects light/dark variables and desktop preference

Custom theme CSS is expected to adapt through variables and the existing `theme-light` / `theme-dark` classes.

## Provider Design

The provider owns desktop preference integration only. It does not know about `auto`.

Provider API:

- `snapshot() -> ThemePreferenceSnapshot`
- `apply(preference: ThemePreference) -> Result<()>`

Runtime types:

- `ThemePreference`
  - `Light`
  - `Dark`
- `ThemePreferenceSnapshot`
  - current effective preference
  - backend identity
  - writable capability

Backend strategy:

- primary backend: GNOME desktop interface color scheme via GSettings
- read support may additionally observe the desktop preference through the same backend

The initial implementation should target the setting that GTK/libadwaita apps commonly follow:

- `org.gnome.desktop.interface color-scheme`

This is a desktop-wide preference, not a Glimpse-only CSS flag. It should be documented as “effective for apps that follow the standard desktop dark-style preference.”

If the backend is unavailable or not writable, the provider returns an error and the service reports degraded health.

## Service Design

The service owns requested config, effective mode, backend health, and scheduling.

Public API:

- `ThemeServiceHandle::new(config: ThemeConfig) -> Self`
- `subscribe() -> watch::Receiver<ThemeState>`
- `send(ThemeCommand)`

Commands:

- `ApplyConfig(ThemeConfig)`
- `Refresh`

State:

- `ThemeState`
  - `health: ThemeHealth`
  - `config: ThemeConfig`
  - `effective_mode: ThemePreference`
  - `source: ThemeSource`
  - `last_applied_mode: Option<ThemePreference>`

Supporting enums:

- `ThemeHealth`
  - `Starting`
  - `Ready`
  - `Degraded { message: String }`
- `ThemeSource`
  - `Manual`
  - `SolarAuto`

Service rules:

- `light` maps directly to effective `Light`
- `dark` maps directly to effective `Dark`
- `auto` resolves solar times and chooses light or dark from the current solar phase
- the requested config is always persisted, even when apply fails
- the service only applies the desktop preference when the effective mode changes
- unchanged state is not republished

## Solar Integration

`auto` reuses the shared solar provider already introduced for Night Light.

The theme service depends on:

- `location::provider::LocationProvider`
- `solar::provider::SolarTimesProvider`

The service does not poll every second. It should:

- resolve current solar times
- determine the current effective mode
- compute the next solar boundary
- sleep until the next boundary with a small safety margin
- recompute after wake-up or on explicit refresh/config changes

If solar times cannot be resolved:

- the service keeps the last applied effective mode
- health becomes degraded
- the requested config stays `Auto`

## Root App Wiring

`ServicesHandle` gains:

- `theme: ThemeServiceHandle`

`Services::new(...)` accepts the current `ThemeConfig` and constructs the theme service once.

`App` wiring changes:

- on startup, pass `config.theme.clone()` into `Services::new(...)`
- subscribe to `ThemeState`
- use `state.effective_mode` instead of `config.theme.mode` when applying local `theme-light` / `theme-dark` classes
- on config reload, send `ThemeCommand::ApplyConfig(new_config.theme.clone())`

`App` should keep using `theme.name` to choose the CSS file, but local mode classes should reflect the service’s effective mode.

## Behavior And Failure Handling

Expected behavior:

- manual `light` forces desktop light preference
- manual `dark` forces desktop dark preference
- `auto` tracks solar day/night and updates the desktop preference only on boundary changes
- Glimpse windows mirror the same effective light/dark mode locally

Failure handling:

- provider apply failure
  - keep requested config
  - mark health degraded
  - preserve last known applied mode
- solar resolution failure in `auto`
  - keep requested config
  - keep last known effective mode
  - mark health degraded
- external desktop preference change
  - allowed
  - the service may observe it on refresh, but configured policy wins on the next scheduled/config-driven apply

The service should log:

- startup
- backend selection
- config changes
- effective mode transitions
- apply failures
- solar resolution failures in `auto`

## Testing

Add targeted tests for:

- config parsing for `light`, `dark`, `auto`
- service resolution from requested config to effective mode
- `auto` day/night switching using mocked solar times
- provider apply failures preserving requested config
- unchanged effective mode not republishing state
- root-app mapping of `ThemeState.effective_mode` to local CSS classes

Mock-friendly design is required:

- the service should depend on a provider trait and a solar-times source trait
- tests should not require real GSettings or real solar lookups

## Implementation Notes

Important boundaries:

- the provider owns desktop preference IO
- the service owns policy and timing
- the app owns only wiring and local CSS synchronization

This should follow the same general pattern used by other Glimpse domains:

- provider
- service
- cloneable service handle with `subscribe()` and `send()`

Night Light and Theme share solar inputs, but they remain separate domains.
