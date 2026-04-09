# Session Applet Refactor Design

**Date:** 2026-04-09

**Issue:** `glimpse-xq8`

## Goal

Refactor the session applet to stop using `glimpse_client`, move its non-UI behavior onto `glimpse::providers::session_actions`, add the missing shared provider features needed for the existing UI, and make every session-ending or power action use an Adwaita confirmation dialog.

## Current Problems

- The session applet still depends on `glimpse_client` instead of the shared provider layer.
- The popover hardcodes action rows and local host/session probing logic instead of rendering from typed provider state.
- `Log Out` is wired to `power.lock`, which is incorrect behavior.
- Confirmation dialogs use `gtk::MessageDialog` instead of the app's Adwaita dialog style.
- The applet has a popover but does not apply the expected `hoverable` class.
- Capability failures do not flow through a typed shared model, so the applet cannot clearly distinguish available and unavailable actions.

## Architecture

The refactor keeps the session applet self-contained and does not add a new app-level service. The panel will pass the system `zbus::Connection` into the applet, matching existing shared-provider usage patterns such as battery and power. The applet will create `glimpse::providers::session_actions::SessionActions::with_connection(system_conn)` and use that shared provider for all session and power actions.

The shared non-UI boundary remains in `glimpse::providers::session_actions`. That provider will be extended to expose:

- action capabilities for the session popover rows;
- real logout support instead of the current broken lock call;
- current-session metadata needed for the popover hero/subtitle so the panel no longer mixes UI code with host/session probing.

The panel applet becomes a thin Relm4 wrapper that:

- loads typed provider state on startup;
- renders the popover from that state;
- shows Adwaita confirmation dialogs before invoking any action;
- calls provider methods asynchronously and logs failures without panicking.

## Provider Design

`glimpse::providers::session_actions` should remain the single shared owner of session-related action logic. It already owns capability probing and the lock/suspend/hibernate/reboot/power-off commands. It should be extended rather than replaced.

### Required additions

- Add a typed snapshot struct that packages:
  - session action capabilities;
  - current user label;
  - hostname;
  - subtitle text or the source fields needed to build it cleanly.
- Add a real `logout()` method implemented against the real session backend.
- Add any small typed enums/structs needed to represent current-session metadata without leaking panel-only formatting concerns into the UI.

### Capability behavior

- Backend discovery or capability query failures must degrade to unavailable state, not crash.
- The provider should keep the existing defensive behavior for unavailable login1 access.
- The applet should not guess availability on its own.

## Applet Behavior

The applet root keeps the current overall UX shape, but the data flow changes.

### Input and wiring

- Replace `glimpse_client` in `SessionInit` with the system `zbus::Connection`.
- Construct `SessionActions::with_connection(system_conn)` during applet startup.
- Load provider snapshot asynchronously and push typed messages back into the component.

### Panel root

- Keep the compact label presentation in the panel.
- Add the `hoverable` class because the applet has a popover.

### Popover

The popover should render typed action rows rather than hardcoded imperative branches. The visible actions remain:

- Lock Screen
- Log Out
- Suspend
- Hibernate
- Restart
- Shut Down

Rows should be derived from config plus provider capabilities:

- config decides whether a row is eligible to appear at all;
- provider capabilities decide whether the action is enabled;
- unavailable actions must never be clickable.

## Confirmation Dialogs

Every action must display an Adwaita confirmation dialog before execution.

This includes:

- Lock Screen
- Log Out
- Suspend
- Hibernate
- Restart
- Shut Down

The session applet should stop using `gtk::MessageDialog`. Confirmation should be handled through an Adwaita dialog attached to the applet’s UI host/root window so the UX matches the rest of the app’s dialog styling.

Behavior requirements:

- if the user cancels, no provider method is called;
- if the user accepts, the popover closes and the provider method is invoked;
- if the provider call fails, log the failure and do not panic.

## Hero Metadata

The current popover hero shows:

- current user name;
- hostname;
- uptime-derived subtitle text.

That metadata is currently assembled inside the popover file. The refactor moves that responsibility into the shared provider boundary so the UI consumes typed session snapshot data instead of probing local files directly.

## Failure Handling

- Provider construction or capability load failure should produce an unavailable snapshot rather than aborting applet init.
- Individual action failures should be logged with action context.
- The popover should remain stable even if some actions are unavailable.
- Unknown backend capability values should remain representable and should degrade safely in the UI.

## Testing and Verification

Provider coverage should be expanded to include the new shared behavior, especially:

- logout behavior wiring;
- current-session metadata parsing and fallback behavior;
- capability mapping and defensive degradation paths.

Applet verification should include:

- compile checks proving the session applet no longer references `glimpse_client`;
- compile checks for the new `SessionInit` and popover wiring;
- a quick review that all confirmation flows use Adwaita dialogs.

## File Impact

Expected primary files:

- `glimpse/src/providers/session_actions.rs`
- `glimpse/src/dbus/login1.rs`
- `glimpse-panel/src/applets/session/applet.rs`
- `glimpse-panel/src/applets/session/popover.rs`
- `glimpse-panel/src/applets/session/mod.rs`
- `glimpse-panel/src/applets/mod.rs`

Possible secondary files:

- `glimpse-panel/src/app.rs` if the chosen Adwaita dialog host requires a small app-level helper pattern already used elsewhere
- `theme.css` only if the refactor reveals missing session-app CSS states; no styles should be hardcoded in Rust

## Non-Goals

- No new global session service handle in this refactor.
- No unrelated redesign of the session applet visuals.
- No migration of unrelated applets away from their current data sources.
