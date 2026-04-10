use std::{error::Error, future::Future, time::Duration};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    network::protocol::{
        NetworkActiveAction, NetworkPrompt, NetworkPromptId, NetworkPromptKind, NetworkPromptReply,
        NetworkServiceCommand, NetworkServiceHealth, NetworkServiceState,
    },
    network::secret_agent::NetworkSecretAgent,
    providers::network::{NetworkProvider, NetworkProviderEvent, NetworkSnapshot, WifiAccessPoint},
};

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

const STARTUP_SCAN_ATTEMPTS: usize = 4;
const STARTUP_SCAN_SETTLE_DELAY: Duration = Duration::from_secs(2);
const STARTUP_SCAN_INTERVAL: Duration = Duration::from_secs(3);

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

fn allocate_prompt_id(next_prompt_id: &mut u64) -> NetworkPromptId {
    let id = NetworkPromptId(*next_prompt_id);
    *next_prompt_id += 1;
    id
}

#[derive(Clone)]
pub struct NetworkServiceHandle {
    commands: mpsc::Sender<NetworkServiceCommand>,
    state: watch::Receiver<NetworkServiceState>,
}

impl NetworkServiceHandle {
    pub fn new(system: zbus::Connection) -> Self {
        let (state_tx, state) = watch::channel(NetworkServiceState {
            health: NetworkServiceHealth::Starting,
            snapshot: Default::default(),
            prompt: None,
            active_action: None,
            scanning: false,
        });
        let (commands, cmd_rx) = mpsc::channel(64);

        tokio::spawn(async move {
            run_network_service(system, state_tx, cmd_rx).await;
        });

        Self { commands, state }
    }

    pub fn subscribe(&self) -> watch::Receiver<NetworkServiceState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: NetworkServiceCommand,
    ) -> Result<(), mpsc::error::SendError<NetworkServiceCommand>> {
        self.commands.send(command).await
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct OpenPopoverCount {
    count: u32,
    interval_secs: u64,
}

impl OpenPopoverCount {
    fn open(&mut self, interval_secs: u64) -> bool {
        self.interval_secs = interval_secs.max(1);
        self.count += 1;
        self.count == 1
    }

    fn close(&mut self) -> bool {
        if self.count == 0 {
            return false;
        }
        self.count -= 1;
        self.count == 0
    }

    fn has_open_popovers(&self) -> bool {
        self.count > 0
    }

    fn interval(&self) -> Duration {
        Duration::from_secs(self.interval_secs.max(1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingPrompt {
    id: NetworkPromptId,
    ssid: String,
}

async fn run_network_service(
    system: zbus::Connection,
    state_tx: watch::Sender<NetworkServiceState>,
    mut cmd_rx: mpsc::Receiver<NetworkServiceCommand>,
) {
    let provider = NetworkProvider::new(system.clone());
    let mut attempt = 0u32;
    let mut open_popovers = OpenPopoverCount::default();

    loop {
        attempt += 1;
        let _ = state_tx.send_modify(|state| {
            state.health = if attempt == 1 {
                NetworkServiceHealth::Starting
            } else {
                NetworkServiceHealth::Reconnecting { attempt }
            };
        });

        match run_connected(
            system.clone(),
            provider.clone(),
            state_tx.clone(),
            &mut cmd_rx,
            &mut open_popovers,
        )
        .await
        {
            Ok(()) => break,
            Err(error) => {
                tracing::warn!(error = %error, attempt, "network service: worker failed");
                let _ = state_tx.send_modify(|state| {
                    state.health = NetworkServiceHealth::Degraded {
                        message: error.to_string(),
                    };
                });
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn run_connected(
    system: zbus::Connection,
    provider: NetworkProvider,
    state_tx: watch::Sender<NetworkServiceState>,
    cmd_rx: &mut mpsc::Receiver<NetworkServiceCommand>,
    open_popovers: &mut OpenPopoverCount,
) -> ServiceResult<()> {
    let secret_agent = NetworkSecretAgent;
    secret_agent
        .register(&system)
        .await
        .map_err(|error| -> ServiceError {
            format!("failed to register network secret agent: {error}").into()
        })?;

    let cancel = CancellationToken::new();
    let (event_tx, mut event_rx) = mpsc::channel(32);
    let mut listener = tokio::spawn({
        let provider = provider.clone();
        let cancel = cancel.clone();
        async move { provider.listen(event_tx, cancel).await }
    });

    refresh_snapshot(&provider, &state_tx).await?;
    let _ = state_tx.send_modify(|state| state.health = NetworkServiceHealth::Ready);

    let mut pending_prompt: Option<PendingPrompt> = None;
    let mut next_prompt_id = 1u64;
    let mut scan_cancel = None;
    let mut startup_scan_cancel = None;
    if should_startup_scan(&state_tx.borrow().snapshot, open_popovers) {
        tracing::info!("network service: starting startup scan warm-up");
        startup_scan_cancel = Some(start_startup_scan_task(provider.clone(), state_tx.clone()));
    }
    if open_popovers.has_open_popovers() {
        scan_cancel = Some(start_scan_task(
            provider.clone(),
            state_tx.clone(),
            open_popovers.interval(),
        ));
    }

    let result = loop {
        tokio::select! {
            maybe_event = event_rx.recv() => {
                match maybe_event {
                    Some(NetworkProviderEvent::Changed { reason }) => {
                        tracing::info!(reason = %reason, "network service: provider changed");
                        if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                            tracing::warn!(error = %error, "network service: refresh failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = NetworkServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        } else {
                            reconcile_active_action(&state_tx);
                            let _ = state_tx.send_modify(|state| state.health = NetworkServiceHealth::Ready);
                        }
                    }
                    None => break Err(service_error("network provider event channel closed")),
                }
            }
            maybe_command = cmd_rx.recv() => {
                match maybe_command {
                    Some(command) => {
                        handle_command(
                            &provider,
                            &state_tx,
                            open_popovers,
                            &mut pending_prompt,
                            &mut next_prompt_id,
                            &mut scan_cancel,
                            &mut startup_scan_cancel,
                            command,
                        ).await?;
                    }
                    None => break Ok(()),
                }
            }
            join = &mut listener => {
                break match join {
                    Ok(Ok(())) => Err(service_error("network listener exited")),
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(service_error(format!("network listener task failed: {error}"))),
                };
            }
        }
    };

    cancel.cancel();
    let _ = secret_agent.unregister(&system).await;
    if let Some(startup_scan_cancel) = startup_scan_cancel.take() {
        startup_scan_cancel.cancel();
    }
    if let Some(scan_cancel) = scan_cancel.take() {
        scan_cancel.cancel();
    }
    result
}

async fn handle_command(
    provider: &NetworkProvider,
    state_tx: &watch::Sender<NetworkServiceState>,
    open_popovers: &mut OpenPopoverCount,
    pending_prompt: &mut Option<PendingPrompt>,
    next_prompt_id: &mut u64,
    scan_cancel: &mut Option<CancellationToken>,
    startup_scan_cancel: &mut Option<CancellationToken>,
    command: NetworkServiceCommand,
) -> ServiceResult<()> {
    match command {
        NetworkServiceCommand::SetWifiEnabled(enabled) => {
            spawn_action(
                provider.clone(),
                state_tx.clone(),
                Some(NetworkActiveAction::SetWifiEnabled(enabled)),
                move |provider| async move {
                    provider.set_wifi_enabled(enabled).await.map_err(Into::into)
                },
            );
            Ok(())
        }
        NetworkServiceCommand::StartScanning { interval_secs } => {
            let needs_start = open_popovers.open(interval_secs);
            if needs_start {
                if let Some(existing_cancel) = startup_scan_cancel.take() {
                    existing_cancel.cancel();
                }
                if let Some(existing_cancel) = scan_cancel.take() {
                    existing_cancel.cancel();
                }
                *scan_cancel = Some(start_scan_task(
                    provider.clone(),
                    state_tx.clone(),
                    open_popovers.interval(),
                ));
            }
            Ok(())
        }
        NetworkServiceCommand::StopScanning => {
            if open_popovers.close() {
                if let Some(existing_cancel) = scan_cancel.take() {
                    existing_cancel.cancel();
                }
            }
            Ok(())
        }
        NetworkServiceCommand::RequestScan => {
            spawn_action(
                provider.clone(),
                state_tx.clone(),
                Some(NetworkActiveAction::Scan),
                move |provider| async move {
                    provider.request_scan().await?;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    Ok(())
                },
            );
            Ok(())
        }
        NetworkServiceCommand::ConnectWifi { ssid } => {
            let access_point = state_tx
                .borrow()
                .snapshot
                .wifi_access_points
                .iter()
                .find(|access_point| access_point.ssid == ssid)
                .cloned();

            match access_point {
                Some(access_point) if needs_password_prompt(&access_point) => {
                    let prompt_id = allocate_prompt_id(next_prompt_id);
                    *pending_prompt = Some(PendingPrompt {
                        id: prompt_id,
                        ssid: access_point.ssid.clone(),
                    });
                    let _ = state_tx.send_modify(|state| {
                        state.prompt = Some(NetworkPrompt {
                            id: prompt_id,
                            kind: NetworkPromptKind::WifiPassword {
                                ssid: access_point.ssid.clone(),
                            },
                        });
                    });
                    Ok(())
                }
                Some(access_point) => {
                    let ssid = access_point.ssid.clone();
                    spawn_action(
                        provider.clone(),
                        state_tx.clone(),
                        Some(NetworkActiveAction::ConnectWifi { ssid: ssid.clone() }),
                        move |provider| async move {
                            provider.connect(&ssid, None).await.map_err(Into::into)
                        },
                    );
                    Ok(())
                }
                None => Err(service_error(format!("unknown wifi network: {ssid}"))),
            }
        }
        NetworkServiceCommand::ConnectSaved { uuid } => {
            spawn_action(
                provider.clone(),
                state_tx.clone(),
                Some(NetworkActiveAction::ConnectSaved { uuid: uuid.clone() }),
                move |provider| async move { provider.connect_uuid(&uuid).await.map_err(Into::into) },
            );
            Ok(())
        }
        NetworkServiceCommand::Disconnect { uuid } => {
            spawn_action(
                provider.clone(),
                state_tx.clone(),
                Some(NetworkActiveAction::Disconnect { uuid: uuid.clone() }),
                move |provider| async move { provider.disconnect(&uuid).await.map_err(Into::into) },
            );
            Ok(())
        }
        NetworkServiceCommand::Forget { uuid } => {
            spawn_action(
                provider.clone(),
                state_tx.clone(),
                Some(NetworkActiveAction::Forget { uuid: uuid.clone() }),
                move |provider| async move { provider.forget(&uuid).await.map_err(Into::into) },
            );
            Ok(())
        }
        NetworkServiceCommand::PromptReply { id, reply } => {
            let Some(pending) = pending_prompt.take() else {
                tracing::warn!(
                    prompt_id = id.0,
                    "network service: prompt reply with no pending prompt"
                );
                return Ok(());
            };
            if pending.id != id {
                tracing::warn!(
                    prompt_id = id.0,
                    expected = pending.id.0,
                    "network service: prompt id mismatch"
                );
                *pending_prompt = Some(pending);
                return Ok(());
            }

            let _ = state_tx.send_modify(|state| state.prompt = None);
            match reply {
                NetworkPromptReply::SubmitPassword(password) => {
                    let ssid = pending.ssid.clone();
                    spawn_action(
                        provider.clone(),
                        state_tx.clone(),
                        Some(NetworkActiveAction::ConnectWifi { ssid: ssid.clone() }),
                        move |provider| async move {
                            provider
                                .connect(&ssid, Some(password.as_str()))
                                .await
                                .map_err(Into::into)
                        },
                    );
                }
                NetworkPromptReply::Cancel => {}
            }
            Ok(())
        }
    }
}

fn spawn_action<F, Fut>(
    provider: NetworkProvider,
    state_tx: watch::Sender<NetworkServiceState>,
    active_action: Option<NetworkActiveAction>,
    action: F,
) where
    F: FnOnce(NetworkProvider) -> Fut + Send + 'static,
    Fut: Future<Output = ServiceResult<()>> + Send + 'static,
{
    let tracks_scan = matches!(active_action, Some(NetworkActiveAction::Scan));
    let _ = state_tx.send_modify(|state| state.active_action = active_action);
    if tracks_scan {
        set_scanning(&state_tx, true);
    }

    tokio::spawn(async move {
        let result = action(provider.clone()).await;
        if let Err(error) = &result {
            tracing::warn!(error = %error, "network service: action failed");
            let _ = state_tx.send_modify(|state| state.active_action = None);
        }
        if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
            tracing::warn!(error = %error, "network service: failed to refresh after action");
        }
        reconcile_active_action(&state_tx);
        if tracks_scan {
            set_scanning(&state_tx, false);
        }
    });
}

fn start_startup_scan_task(
    provider: NetworkProvider,
    state_tx: watch::Sender<NetworkServiceState>,
) -> CancellationToken {
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    tokio::spawn(async move {
        for attempt in 0..STARTUP_SCAN_ATTEMPTS {
            let snapshot = state_tx.borrow().snapshot.clone();
            if !startup_scan_supported(&snapshot) {
                tracing::info!(
                    attempt = attempt + 1,
                    "network service: startup scan finished early"
                );
                break;
            }

            tracing::info!(
                attempt = attempt + 1,
                total = STARTUP_SCAN_ATTEMPTS,
                "network service: startup scan attempt"
            );
            set_scanning(&state_tx, true);
            if let Err(error) = provider.request_scan().await {
                tracing::warn!(error = %error, "network service: startup scan request failed");
            }

            tokio::select! {
                _ = task_cancel.cancelled() => {
                    set_scanning(&state_tx, false);
                    break;
                },
                _ = tokio::time::sleep(STARTUP_SCAN_SETTLE_DELAY) => {}
            }

            if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                tracing::warn!(error = %error, "network service: startup scan refresh failed");
            }
            set_scanning(&state_tx, false);

            let snapshot = state_tx.borrow().snapshot.clone();
            if !startup_scan_supported(&snapshot) || attempt + 1 == STARTUP_SCAN_ATTEMPTS {
                tracing::info!(
                    attempt = attempt + 1,
                    total = STARTUP_SCAN_ATTEMPTS,
                    "network service: startup scan completed"
                );
                break;
            }

            tokio::select! {
                _ = task_cancel.cancelled() => break,
                _ = tokio::time::sleep(STARTUP_SCAN_INTERVAL) => {}
            }
        }
    });
    cancel
}

fn start_scan_task(
    provider: NetworkProvider,
    state_tx: watch::Sender<NetworkServiceState>,
    interval: Duration,
) -> CancellationToken {
    let cancel = CancellationToken::new();
    let task_cancel = cancel.clone();
    tokio::spawn(async move {
        loop {
            set_scanning(&state_tx, true);
            let _ = provider.request_scan().await;
            tokio::select! {
                _ = task_cancel.cancelled() => {
                    set_scanning(&state_tx, false);
                    break;
                },
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
            let _ = refresh_snapshot(&provider, &state_tx).await;
            set_scanning(&state_tx, false);
            tokio::select! {
                _ = task_cancel.cancelled() => break,
                _ = tokio::time::sleep(interval) => {}
            }
        }
    });
    cancel
}

async fn refresh_snapshot(
    provider: &NetworkProvider,
    state_tx: &watch::Sender<NetworkServiceState>,
) -> ServiceResult<()> {
    let snapshot = provider.scan().await?;
    let _ = state_tx.send_modify(|state| state.snapshot = snapshot);
    Ok(())
}

fn needs_password_prompt(access_point: &WifiAccessPoint) -> bool {
    !access_point.saved && access_point.security != "open" && !access_point.security.is_empty()
}

fn should_startup_scan(snapshot: &NetworkSnapshot, open_popovers: &OpenPopoverCount) -> bool {
    !open_popovers.has_open_popovers() && startup_scan_supported(snapshot)
}

fn startup_scan_supported(snapshot: &NetworkSnapshot) -> bool {
    snapshot.status.wifi_enabled
        && snapshot.status.wifi_hw_enabled
        && snapshot
            .devices
            .iter()
            .any(|device| device.device_type == "wifi")
}

fn reconcile_active_action(state_tx: &watch::Sender<NetworkServiceState>) {
    let _ = state_tx.send_modify(|state| {
        if let Some(active_action) = state.active_action.as_ref() {
            if action_has_reached_observable_state(&state.snapshot, active_action) {
                state.active_action = None;
            }
        }
    });
}

fn action_has_reached_observable_state(
    snapshot: &NetworkSnapshot,
    active_action: &NetworkActiveAction,
) -> bool {
    match active_action {
        NetworkActiveAction::ConnectWifi { ssid } => snapshot.connections.iter().any(|connection| {
            connection.connection_type == "wifi"
                && connection.id == *ssid
                && (connection.state == "activating" || connection.state == "activated")
        }) || snapshot
            .wifi_access_points
            .iter()
            .any(|access_point| access_point.ssid == *ssid && access_point.connected),
        NetworkActiveAction::ConnectSaved { uuid } => snapshot.connections.iter().any(|connection| {
            connection.uuid == *uuid
                && (connection.state == "activating" || connection.state == "activated")
        }),
        NetworkActiveAction::Disconnect { uuid } => !snapshot
            .connections
            .iter()
            .any(|connection| connection.uuid == *uuid),
        _ => true,
    }
}

fn set_scanning(state_tx: &watch::Sender<NetworkServiceState>, scanning: bool) {
    let _ = state_tx.send_modify(|state| state.scanning = scanning);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wifi_startup_snapshot() -> NetworkSnapshot {
        NetworkSnapshot {
            status: crate::providers::network::NetworkStatus {
                wifi_enabled: true,
                wifi_hw_enabled: true,
                ..crate::providers::network::NetworkStatus::default()
            },
            devices: vec![crate::providers::network::NetworkDevice {
                device_type: "wifi".into(),
                ..crate::providers::network::NetworkDevice::default()
            }],
            ..NetworkSnapshot::default()
        }
    }

    #[test]
    fn intermediate_popover_close_does_not_stop_scanning() {
        let mut popovers = OpenPopoverCount {
            count: 2,
            interval_secs: 15,
        };

        assert!(!popovers.close());
        assert_eq!(popovers.count, 1);
    }

    #[test]
    fn last_popover_close_stops_scanning() {
        let mut popovers = OpenPopoverCount {
            count: 1,
            interval_secs: 15,
        };

        assert!(popovers.close());
        assert_eq!(popovers.count, 0);
    }

    #[test]
    fn first_popover_open_starts_scanning() {
        let mut popovers = OpenPopoverCount::default();

        assert!(popovers.open(20));
        assert!(!popovers.open(10));
        assert_eq!(popovers.count, 2);
        assert_eq!(popovers.interval(), Duration::from_secs(10));
    }

    #[test]
    fn startup_scan_runs_for_empty_wifi_snapshot() {
        assert!(should_startup_scan(
            &wifi_startup_snapshot(),
            &OpenPopoverCount::default()
        ));
    }

    #[test]
    fn startup_scan_skips_when_popover_scan_is_active() {
        let mut popovers = OpenPopoverCount::default();
        assert!(popovers.open(15));

        assert!(!should_startup_scan(&wifi_startup_snapshot(), &popovers));
    }

    #[test]
    fn startup_scan_runs_for_non_empty_wifi_snapshot() {
        let mut snapshot = wifi_startup_snapshot();
        snapshot.wifi_access_points.push(WifiAccessPoint {
            ssid: "Office".into(),
            ..WifiAccessPoint::default()
        });

        assert!(should_startup_scan(&snapshot, &OpenPopoverCount::default()));
    }

    #[test]
    fn startup_scan_stops_when_access_points_are_present() {
        let mut snapshot = wifi_startup_snapshot();
        snapshot.wifi_access_points.push(WifiAccessPoint {
            ssid: "Office".into(),
            ..WifiAccessPoint::default()
        });

        assert!(startup_scan_supported(&snapshot));
    }

    #[test]
    fn connect_saved_action_persists_until_connection_is_visible() {
        let snapshot = NetworkSnapshot::default();

        assert!(!action_has_reached_observable_state(
            &snapshot,
            &NetworkActiveAction::ConnectSaved {
                uuid: "uuid-1".into(),
            }
        ));
    }

    #[test]
    fn connect_saved_action_clears_when_connection_starts_activating() {
        let snapshot = NetworkSnapshot {
            connections: vec![crate::providers::network::NetworkConnection {
                uuid: "uuid-1".into(),
                connection_type: "wifi".into(),
                state: "activating".into(),
                ..Default::default()
            }],
            ..Default::default()
        };

        assert!(action_has_reached_observable_state(
            &snapshot,
            &NetworkActiveAction::ConnectSaved {
                uuid: "uuid-1".into(),
            }
        ));
    }

    #[test]
    fn secured_unsaved_access_point_requires_prompt() {
        assert!(needs_password_prompt(&WifiAccessPoint {
            security: "wpa2".into(),
            saved: false,
            ..WifiAccessPoint::default()
        }));
        assert!(!needs_password_prompt(&WifiAccessPoint {
            security: "open".into(),
            saved: false,
            ..WifiAccessPoint::default()
        }));
        assert!(!needs_password_prompt(&WifiAccessPoint {
            security: "wpa2".into(),
            saved: true,
            ..WifiAccessPoint::default()
        }));
    }

    #[test]
    fn allocated_network_prompt_ids_are_monotonic() {
        let mut next_prompt_id = 1;

        let first = allocate_prompt_id(&mut next_prompt_id);
        let second = allocate_prompt_id(&mut next_prompt_id);

        assert_eq!(first, NetworkPromptId(1));
        assert_eq!(second, NetworkPromptId(2));
    }
}
