use std::time::Duration;

use glimpse_core::services::{
    framework::ServiceCommand,
    idle::{self, ActiveListener, Health, IdleHandle, State},
};
use tokio_util::sync::CancellationToken;
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, delegate_noop,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{wl_registry, wl_seat},
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};

const WAYLAND_POLL_INTERVAL: Duration = Duration::from_millis(250);
const WAYLAND_RETRY_DELAY: Duration = Duration::from_secs(2);

pub async fn run(idle: IdleHandle, cancel: CancellationToken) {
    loop {
        match run_inner(idle.clone(), cancel.clone()).await {
            Ok(RunOutcome::Cancelled) => break,
            Err(error) => {
                tracing::warn!(%error, "idle backend failed");
                send_health(
                    &idle,
                    Health::Degraded {
                        message: error.to_string(),
                    },
                );
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(WAYLAND_RETRY_DELAY) => {}
                }
            }
        }
    }
}

enum RunOutcome {
    Cancelled,
}

async fn run_inner(idle: IdleHandle, cancel: CancellationToken) -> anyhow::Result<RunOutcome> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init::<WaylandIdleState>(&conn)?;
    let qh = event_queue.handle();
    let notifier = globals.bind::<ext_idle_notifier_v1::ExtIdleNotifierV1, _, _>(&qh, 1..=2, ())?;
    let seat = globals.bind::<wl_seat::WlSeat, _, _>(&qh, 1..=9, ())?;
    let mut backend_state = WaylandIdleState {
        idle: idle.clone(),
        notifications: vec![],
        registered_generation: None,
    };
    event_queue.roundtrip(&mut backend_state)?;

    tracing::info!("idle backend connected to Wayland idle notify");
    send_health(&idle, Health::Ready);

    let mut state_rx = idle.subscribe();
    sync_notifications(&mut backend_state, &notifier, &seat, &qh, &idle.snapshot());
    let mut tick = tokio::time::interval(WAYLAND_POLL_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                clear_notifications(&mut backend_state);
                return Ok(RunOutcome::Cancelled);
            }
            changed = state_rx.changed() => {
                if changed.is_err() {
                    clear_notifications(&mut backend_state);
                    return Ok(RunOutcome::Cancelled);
                }
                let state = state_rx.borrow().clone();
                if backend_state.registered_generation != Some(state.generation) {
                    sync_notifications(
                        &mut backend_state,
                        &notifier,
                        &seat,
                        &qh,
                        &state,
                    );
                    conn.flush()?;
                }
            }
            _ = tick.tick() => {
                event_queue.roundtrip(&mut backend_state)?;
            }
        }
    }
}

struct WaylandIdleState {
    idle: IdleHandle,
    notifications: Vec<ext_idle_notification_v1::ExtIdleNotificationV1>,
    registered_generation: Option<u64>,
}

fn sync_notifications(
    state: &mut WaylandIdleState,
    notifier: &ext_idle_notifier_v1::ExtIdleNotifierV1,
    seat: &wl_seat::WlSeat,
    qh: &QueueHandle<WaylandIdleState>,
    idle_state: &State,
) {
    clear_notifications(state);

    if !idle_state.enabled {
        tracing::debug!("idle backend: idle policy disabled, no listeners registered");
        state.registered_generation = Some(idle_state.generation);
        return;
    }

    for listener in &idle_state.listeners {
        if let Some(notification) = register_listener(notifier, seat, qh, listener) {
            state.notifications.push(notification);
        }
    }

    tracing::info!(
        generation = idle_state.generation,
        listeners = state.notifications.len(),
        "idle backend registered listeners"
    );
    state.registered_generation = Some(idle_state.generation);
}

fn register_listener(
    notifier: &ext_idle_notifier_v1::ExtIdleNotifierV1,
    seat: &wl_seat::WlSeat,
    qh: &QueueHandle<WaylandIdleState>,
    listener: &ActiveListener,
) -> Option<ext_idle_notification_v1::ExtIdleNotificationV1> {
    let timeout_ms = listener.timeout.saturating_mul(1000).min(u32::MAX as u64) as u32;
    tracing::debug!(
        listener = listener.id,
        timeout = listener.timeout,
        respect_inhibitors = listener.respect_inhibitors,
        "idle backend registering listener"
    );

    if listener.respect_inhibitors {
        Some(notifier.get_idle_notification(timeout_ms, seat, qh, listener.id))
    } else if notifier.version() >= 2 {
        Some(notifier.get_input_idle_notification(timeout_ms, seat, qh, listener.id))
    } else {
        tracing::warn!(
            listener = listener.id,
            timeout = listener.timeout,
            "idle backend cannot ignore inhibitors because ext-idle-notify v2 is unavailable"
        );
        Some(notifier.get_idle_notification(timeout_ms, seat, qh, listener.id))
    }
}

fn clear_notifications(state: &mut WaylandIdleState) {
    for notification in state.notifications.drain(..) {
        notification.destroy();
    }
}

fn send_health(idle: &IdleHandle, health: Health) {
    if let Err(error) = idle.try_send(ServiceCommand::Command(idle::Command::SetBackendHealth(
        health,
    ))) {
        tracing::warn!(%error, "failed to update idle backend health");
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandIdleState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, usize> for WaylandIdleState {
    fn event(
        state: &mut Self,
        _proxy: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        listener: &usize,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let command = match event {
            ext_idle_notification_v1::Event::Idled => idle::Command::ListenerIdle(*listener),
            ext_idle_notification_v1::Event::Resumed => idle::Command::ListenerResume(*listener),
            _ => return,
        };
        if let Err(error) = state.idle.try_send(ServiceCommand::Command(command)) {
            tracing::warn!(listener, %error, "failed to forward idle backend event");
        }
    }
}

delegate_noop!(WaylandIdleState: ignore wl_seat::WlSeat);
delegate_noop!(WaylandIdleState: ignore ext_idle_notifier_v1::ExtIdleNotifierV1);
