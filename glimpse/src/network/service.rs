use std::{error::Error, future::Future, time::Duration};

use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    network::protocol::{
        NetworkActiveAction, NetworkPrompt, NetworkPromptId, NetworkPromptKind, NetworkPromptReply,
        NetworkServiceCommand, NetworkServiceHealth, NetworkServiceState,
    },
    network::secret_agent::NetworkSecretAgent,
    network::provider::{
        NetworkChangeReason, NetworkFailureClassification, NetworkProvider, NetworkProviderEvent,
        NetworkSnapshot, WifiAccessPoint,
    },
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
    path: String,
    submitting: bool,
    active_connection_path: Option<String>,
    connection_uuid: Option<String>,
    settings_path: Option<String>,
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
                        log_network_provider_change(&reason);
                        if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                            tracing::warn!(error = %error, "network service: refresh failed");
                            let _ = state_tx.send_modify(|state| {
                                state.health = NetworkServiceHealth::Degraded {
                                    message: error.to_string(),
                                };
                            });
                        } else {
                            if let Some(reconciliation) =
                                reconcile_pending_prompt(&state_tx, &mut pending_prompt)
                            {
                                match reconciliation {
                                    PendingPromptReconciliation::Save { settings_path } => {
                                        if let Err(error) = provider.save_connection_path(&settings_path).await {
                                            tracing::warn!(error = %error, path = settings_path, "network service: failed to persist wifi profile");
                                        } else if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                                            tracing::warn!(error = %error, "network service: failed to refresh after profile save");
                                        }
                                    }
                                    PendingPromptReconciliation::Delete { settings_path } => {
                                        if let Err(error) = provider.delete_connection_path(&settings_path).await {
                                            tracing::warn!(error = %error, path = settings_path, "network service: failed to delete invalid wifi profile");
                                        } else if let Err(error) = refresh_snapshot(&provider, &state_tx).await {
                                            tracing::warn!(error = %error, "network service: failed to refresh after profile cleanup");
                                        }
                                    }
                                }
                            }
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
        NetworkServiceCommand::ConnectWifi { ssid, path } => {
            let access_point = state_tx
                .borrow()
                .snapshot
                .wifi_access_points
                .iter()
                .find(|access_point| access_point.path == path)
                .cloned();

            match access_point {
                Some(access_point) if needs_password_prompt(&access_point) => {
                    let prompt_id = allocate_prompt_id(next_prompt_id);
                    *pending_prompt = Some(PendingPrompt {
                        id: prompt_id,
                        ssid: access_point.ssid.clone(),
                        path: access_point.path.clone(),
                        submitting: false,
                        active_connection_path: None,
                        connection_uuid: None,
                        settings_path: None,
                    });
                    let _ = state_tx.send_modify(|state| {
                        state.prompt = Some(network_password_prompt(
                            prompt_id,
                            access_point.ssid.clone(),
                            None,
                            false,
                        ));
                    });
                    Ok(())
                }
                Some(access_point) => {
                    let ssid = access_point.ssid.clone();
                    let path = access_point.path.clone();
                    spawn_action(
                        provider.clone(),
                        state_tx.clone(),
                        Some(NetworkActiveAction::ConnectWifi {
                            ssid: ssid.clone(),
                            path: path.clone(),
                        }),
                        move |provider| async move {
                            provider
                                .connect_access_point(&ssid, &path, None)
                                .await
                                .map(|_| ())
                                .map_err(Into::into)
                        },
                    );
                    Ok(())
                }
                None => Err(service_error(format!("unknown wifi network: {ssid} ({path})"))),
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
            let Some(pending) = pending_prompt.as_mut() else {
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
                return Ok(());
            }

            match reply {
                NetworkPromptReply::SubmitPassword(password) => {
                    let ssid = pending.ssid.clone();
                    let path = pending.path.clone();
                    let prompt_id = pending.id;
                    pending.submitting = true;
                    pending.active_connection_path = None;
                    pending.connection_uuid = None;
                    pending.settings_path = None;
                    let _ = state_tx.send_modify(|state| {
                        state.prompt =
                            Some(network_password_prompt(prompt_id, ssid.clone(), None, true));
                        state.active_action = Some(NetworkActiveAction::ConnectWifi {
                            ssid: ssid.clone(),
                            path: path.clone(),
                        });
                    });
                    match provider
                        .connect_access_point(&ssid, &path, Some(password.as_str()))
                        .await
                        .map_err(|error| -> ServiceError { error.into() })
                    {
                        Ok(target) => {
                            pending.active_connection_path = Some(target.active_path);
                            pending.connection_uuid = target.connection_uuid;
                            pending.settings_path = Some(target.settings_path);
                        }
                        Err(error) => {
                            tracing::warn!(error = %error, "network service: action failed");
                            restore_pending_prompt_after_submit_error(
                                state_tx,
                                pending,
                                "Failed to start connection. Try again.",
                            );
                        }
                    }
                    if let Err(error) = refresh_snapshot(provider, state_tx).await {
                        tracing::warn!(error = %error, "network service: failed to refresh after action");
                    }
                    if let Some(reconciliation) = reconcile_pending_prompt(state_tx, pending_prompt)
                    {
                        match reconciliation {
                            PendingPromptReconciliation::Save { settings_path } => {
                                if let Err(error) =
                                    provider.save_connection_path(&settings_path).await
                                {
                                    tracing::warn!(error = %error, path = settings_path, "network service: failed to persist wifi profile");
                                } else if let Err(error) =
                                    refresh_snapshot(provider, state_tx).await
                                {
                                    tracing::warn!(error = %error, "network service: failed to refresh after profile save");
                                }
                            }
                            PendingPromptReconciliation::Delete { settings_path } => {
                                if let Err(error) =
                                    provider.delete_connection_path(&settings_path).await
                                {
                                    tracing::warn!(error = %error, path = settings_path, "network service: failed to delete invalid wifi profile");
                                } else if let Err(error) =
                                    refresh_snapshot(provider, state_tx).await
                                {
                                    tracing::warn!(error = %error, "network service: failed to refresh after profile cleanup");
                                }
                            }
                        }
                    }
                    reconcile_active_action(state_tx);
                }
                NetworkPromptReply::Cancel => {
                    *pending_prompt = None;
                    let _ = state_tx.send_modify(|state| state.prompt = None);
                }
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

fn network_password_prompt(
    id: NetworkPromptId,
    ssid: String,
    error_message: Option<String>,
    submitting: bool,
) -> NetworkPrompt {
    NetworkPrompt {
        id,
        kind: NetworkPromptKind::WifiPassword { ssid },
        error_message,
        submitting,
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingPromptReconciliation {
    Save { settings_path: String },
    Delete { settings_path: String },
}

fn reconcile_pending_prompt(
    state_tx: &watch::Sender<NetworkServiceState>,
    pending_prompt: &mut Option<PendingPrompt>,
) -> Option<PendingPromptReconciliation> {
    let Some(pending) = pending_prompt.as_mut() else {
        return None;
    };
    if !pending.submitting {
        return None;
    }

    let snapshot = state_tx.borrow().snapshot.clone();
    if wifi_connection_visible(&snapshot, pending) {
        let settings_path = pending.settings_path.clone();
        *pending_prompt = None;
        let _ = state_tx.send_modify(|state| state.prompt = None);
        return settings_path
            .map(|settings_path| PendingPromptReconciliation::Save { settings_path });
    }

    let Some(classification) = pending_prompt_failure(&snapshot, pending) else {
        if pending_prompt_disappeared(&snapshot, pending) {
            let settings_path = pending.settings_path.clone();
            restore_pending_prompt_after_submit_error(
                state_tx,
                pending,
                "Connection failed. Check the password and try again.",
            );
            return settings_path
                .map(|settings_path| PendingPromptReconciliation::Delete { settings_path });
        }
        return None;
    };

    let cleanup_path = pending.settings_path.clone();
    restore_pending_prompt_after_submit_error(
        state_tx,
        pending,
        prompt_error_message(&classification),
    );
    cleanup_path.map(|settings_path| PendingPromptReconciliation::Delete { settings_path })
}

fn wifi_connection_visible(snapshot: &NetworkSnapshot, pending: &PendingPrompt) -> bool {
    snapshot.connections.iter().any(|connection| {
        pending_prompt_matches_connection(pending, connection) && connection.state == "activated"
    }) || snapshot
        .wifi_access_points
        .iter()
        .any(|access_point| access_point.ssid == pending.ssid && access_point.connected)
}

fn pending_prompt_failure(
    snapshot: &NetworkSnapshot,
    pending: &PendingPrompt,
) -> Option<NetworkFailureClassification> {
    snapshot
        .connections
        .iter()
        .find(|connection| pending_prompt_matches_connection(pending, connection))
        .and_then(|connection| connection.failure.clone())
}

fn pending_prompt_disappeared(snapshot: &NetworkSnapshot, pending: &PendingPrompt) -> bool {
    let has_tracked_identity =
        pending.active_connection_path.is_some() || pending.connection_uuid.is_some();
    has_tracked_identity
        && !snapshot
            .connections
            .iter()
            .any(|connection| pending_prompt_matches_connection(pending, connection))
        && !snapshot
            .wifi_access_points
            .iter()
            .any(|access_point| access_point.ssid == pending.ssid && access_point.connected)
}

fn pending_prompt_matches_connection(
    pending: &PendingPrompt,
    connection: &crate::network::provider::NetworkConnection,
) -> bool {
    if connection.connection_type != "wifi" {
        return false;
    }

    if let Some(active_path) = pending.active_connection_path.as_deref() {
        if connection.active_path == active_path {
            return true;
        }
    }

    if let Some(connection_uuid) = pending.connection_uuid.as_deref() {
        if connection.uuid == connection_uuid {
            return true;
        }
    }

    connection.id == pending.ssid
}

fn restore_pending_prompt_after_submit_error(
    state_tx: &watch::Sender<NetworkServiceState>,
    pending: &mut PendingPrompt,
    message: impl Into<String>,
) {
    pending.submitting = false;
    pending.active_connection_path = None;
    pending.connection_uuid = None;
    pending.settings_path = None;
    let prompt_id = pending.id;
    let ssid = pending.ssid.clone();
    let message = message.into();
    let _ = state_tx.send_modify(|state| {
        state.prompt = Some(network_password_prompt(
            prompt_id,
            ssid.clone(),
            Some(message.clone()),
            false,
        ));
        if matches!(
            state.active_action.as_ref(),
            Some(NetworkActiveAction::ConnectWifi {
                ssid: active_ssid,
                ..
            }) if active_ssid == &ssid
        ) {
            state.active_action = None;
        }
    });
}

fn prompt_error_message(classification: &NetworkFailureClassification) -> &'static str {
    match classification {
        NetworkFailureClassification::AuthenticationFailed => "Incorrect password. Try again.",
        NetworkFailureClassification::MissingSecrets => "Password required. Try again.",
        NetworkFailureClassification::Timeout => "Connection timed out. Try again.",
        NetworkFailureClassification::NetworkNotFound => {
            "Network not found. Refresh and try again."
        }
        NetworkFailureClassification::ConfigurationFailed => {
            "Connection failed. Check the password and try again."
        }
        NetworkFailureClassification::ConnectionRemoved => "Connection was removed. Try again.",
        NetworkFailureClassification::Disconnected => "Connection was interrupted. Try again.",
    }
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
        NetworkActiveAction::ConnectWifi { ssid, path } => {
            snapshot.connections.iter().any(|connection| {
                connection.connection_type == "wifi"
                    && connection.id == *ssid
                    && (connection.state == "activating" || connection.state == "activated")
            }) || snapshot
                .wifi_access_points
                .iter()
                .any(|access_point| access_point.path == *path && access_point.connected)
        }
        NetworkActiveAction::ConnectSaved { uuid } => {
            snapshot.connections.iter().any(|connection| {
                connection.uuid == *uuid
                    && (connection.state == "activating" || connection.state == "activated")
            })
        }
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

fn network_provider_change_logs_at_info(_reason: &NetworkChangeReason) -> bool {
    false
}

fn log_network_provider_change(reason: &NetworkChangeReason) {
    if network_provider_change_logs_at_info(reason) {
        tracing::info!(reason = %reason, "network service: provider changed");
    } else {
        tracing::debug!(reason = %reason, "network service: provider changed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wifi_startup_snapshot() -> NetworkSnapshot {
        NetworkSnapshot {
            status: crate::network::provider::NetworkStatus {
                wifi_enabled: true,
                wifi_hw_enabled: true,
                ..crate::network::provider::NetworkStatus::default()
            },
            devices: vec![crate::network::provider::NetworkDevice {
                device_type: "wifi".into(),
                ..crate::network::provider::NetworkDevice::default()
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
            connections: vec![crate::network::provider::NetworkConnection {
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
    fn provider_change_logs_are_debug_only() {
        assert!(!network_provider_change_logs_at_info(
            &NetworkChangeReason::PropertiesChanged
        ));
        assert!(!network_provider_change_logs_at_info(
            &NetworkChangeReason::Mixed
        ));
    }

    #[test]
    fn allocated_network_prompt_ids_are_monotonic() {
        let mut next_prompt_id = 1;

        let first = allocate_prompt_id(&mut next_prompt_id);
        let second = allocate_prompt_id(&mut next_prompt_id);

        assert_eq!(first, NetworkPromptId(1));
        assert_eq!(second, NetworkPromptId(2));
    }

    #[test]
    fn submitting_prompt_turns_auth_failure_into_inline_error() {
        let (state_tx, _state_rx) = watch::channel(NetworkServiceState {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                connections: vec![crate::network::provider::NetworkConnection {
                    active_path: "/active/1".into(),
                    id: "Skylink".into(),
                    connection_type: "wifi".into(),
                    failure: Some(NetworkFailureClassification::AuthenticationFailed),
                    ..crate::network::provider::NetworkConnection::default()
                }],
                ..NetworkSnapshot::default()
            },
            prompt: Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                None,
                true,
            )),
            active_action: Some(NetworkActiveAction::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            }),
            scanning: false,
        });
        let mut pending_prompt = Some(PendingPrompt {
            id: NetworkPromptId(1),
            ssid: "Skylink".into(),
            path: "/ap/1".into(),
            submitting: true,
            active_connection_path: Some("/active/1".into()),
            connection_uuid: None,
            settings_path: Some("/settings/1".into()),
        });

        reconcile_pending_prompt(&state_tx, &mut pending_prompt);

        let state = state_tx.borrow().clone();
        assert_eq!(
            state.prompt,
            Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                Some("Incorrect password. Try again.".into()),
                false,
            ))
        );
        assert_eq!(state.active_action, None);
        assert!(pending_prompt.is_some());
        assert!(!pending_prompt.as_ref().unwrap().submitting);
    }

    #[test]
    fn submitting_prompt_ignores_unscoped_wifi_device_failures() {
        let original_prompt =
            network_password_prompt(NetworkPromptId(1), "Skylink".into(), None, true);
        let (state_tx, _state_rx) = watch::channel(NetworkServiceState {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                devices: vec![crate::network::provider::NetworkDevice {
                    device_type: "wifi".into(),
                    failure: Some(NetworkFailureClassification::AuthenticationFailed),
                    ..crate::network::provider::NetworkDevice::default()
                }],
                ..NetworkSnapshot::default()
            },
            prompt: Some(original_prompt.clone()),
            active_action: Some(NetworkActiveAction::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            }),
            scanning: false,
        });
        let mut pending_prompt = Some(PendingPrompt {
            id: NetworkPromptId(1),
            ssid: "Skylink".into(),
            path: "/ap/1".into(),
            submitting: true,
            active_connection_path: None,
            connection_uuid: None,
            settings_path: None,
        });

        reconcile_pending_prompt(&state_tx, &mut pending_prompt);

        let state = state_tx.borrow().clone();
        assert_eq!(state.prompt, Some(original_prompt));
        assert_eq!(
            state.active_action,
            Some(NetworkActiveAction::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            })
        );
        assert_eq!(
            pending_prompt,
            Some(PendingPrompt {
                id: NetworkPromptId(1),
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
                submitting: true,
                active_connection_path: None,
                connection_uuid: None,
                settings_path: None,
            })
        );
    }

    #[test]
    fn submitting_prompt_stays_open_while_connection_is_only_activating() {
        let (state_tx, _state_rx) = watch::channel(NetworkServiceState {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                connections: vec![crate::network::provider::NetworkConnection {
                    active_path: "/active/1".into(),
                    id: "Skylink".into(),
                    connection_type: "wifi".into(),
                    state: "activating".into(),
                    ..crate::network::provider::NetworkConnection::default()
                }],
                ..NetworkSnapshot::default()
            },
            prompt: Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                None,
                true,
            )),
            active_action: Some(NetworkActiveAction::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            }),
            scanning: false,
        });
        let mut pending_prompt = Some(PendingPrompt {
            id: NetworkPromptId(1),
            ssid: "Skylink".into(),
            path: "/ap/1".into(),
            submitting: true,
            active_connection_path: Some("/active/1".into()),
            connection_uuid: None,
            settings_path: Some("/settings/1".into()),
        });

        reconcile_pending_prompt(&state_tx, &mut pending_prompt);

        let state = state_tx.borrow().clone();
        assert_eq!(
            state.prompt,
            Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                None,
                true,
            ))
        );
        assert_eq!(
            pending_prompt,
            Some(PendingPrompt {
                id: NetworkPromptId(1),
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
                submitting: true,
                active_connection_path: Some("/active/1".into()),
                connection_uuid: None,
                settings_path: Some("/settings/1".into()),
            })
        );
    }

    #[test]
    fn submitting_prompt_clears_after_connection_is_activated() {
        let (state_tx, _state_rx) = watch::channel(NetworkServiceState {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                connections: vec![crate::network::provider::NetworkConnection {
                    active_path: "/active/1".into(),
                    id: "Skylink".into(),
                    connection_type: "wifi".into(),
                    state: "activated".into(),
                    ..crate::network::provider::NetworkConnection::default()
                }],
                ..NetworkSnapshot::default()
            },
            prompt: Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                None,
                true,
            )),
            active_action: Some(NetworkActiveAction::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            }),
            scanning: false,
        });
        let mut pending_prompt = Some(PendingPrompt {
            id: NetworkPromptId(1),
            ssid: "Skylink".into(),
            path: "/ap/1".into(),
            submitting: true,
            active_connection_path: Some("/active/1".into()),
            connection_uuid: None,
            settings_path: Some("/settings/1".into()),
        });

        let reconciliation = reconcile_pending_prompt(&state_tx, &mut pending_prompt);

        let state = state_tx.borrow().clone();
        assert_eq!(
            reconciliation,
            Some(PendingPromptReconciliation::Save {
                settings_path: "/settings/1".into(),
            })
        );
        assert_eq!(state.prompt, None);
        assert_eq!(pending_prompt, None);
    }

    #[test]
    fn retryable_auth_failure_requests_cleanup_of_created_profile() {
        let (state_tx, _state_rx) = watch::channel(NetworkServiceState {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                connections: vec![crate::network::provider::NetworkConnection {
                    active_path: "/active/1".into(),
                    id: "Skylink".into(),
                    connection_type: "wifi".into(),
                    state: "unknown".into(),
                    failure: Some(NetworkFailureClassification::AuthenticationFailed),
                    ..crate::network::provider::NetworkConnection::default()
                }],
                ..NetworkSnapshot::default()
            },
            prompt: Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                None,
                true,
            )),
            active_action: Some(NetworkActiveAction::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            }),
            scanning: false,
        });
        let mut pending_prompt = Some(PendingPrompt {
            id: NetworkPromptId(1),
            ssid: "Skylink".into(),
            path: "/ap/1".into(),
            submitting: true,
            active_connection_path: Some("/active/1".into()),
            connection_uuid: Some("uuid-1".into()),
            settings_path: Some("/settings/1".into()),
        });

        let reconciliation = reconcile_pending_prompt(&state_tx, &mut pending_prompt);

        assert_eq!(
            reconciliation,
            Some(PendingPromptReconciliation::Delete {
                settings_path: "/settings/1".into(),
            })
        );
    }

    #[test]
    fn disappearing_tracked_connection_finishes_prompt_with_retryable_error() {
        let original_prompt =
            network_password_prompt(NetworkPromptId(1), "Skylink".into(), None, true);
        let (state_tx, _state_rx) = watch::channel(NetworkServiceState {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot::default(),
            prompt: Some(original_prompt),
            active_action: Some(NetworkActiveAction::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            }),
            scanning: false,
        });
        let mut pending_prompt = Some(PendingPrompt {
            id: NetworkPromptId(1),
            ssid: "Skylink".into(),
            path: "/ap/1".into(),
            submitting: true,
            active_connection_path: Some("/active/1".into()),
            connection_uuid: Some("uuid-1".into()),
            settings_path: Some("/settings/1".into()),
        });

        let reconciliation = reconcile_pending_prompt(&state_tx, &mut pending_prompt);

        let state = state_tx.borrow().clone();
        assert_eq!(
            reconciliation,
            Some(PendingPromptReconciliation::Delete {
                settings_path: "/settings/1".into(),
            })
        );
        assert_eq!(
            state.prompt,
            Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                Some("Connection failed. Check the password and try again.".into()),
                false,
            ))
        );
        assert_eq!(state.active_action, None);
    }

    #[test]
    fn activated_prompt_requests_profile_persistence() {
        let (state_tx, _state_rx) = watch::channel(NetworkServiceState {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot {
                connections: vec![crate::network::provider::NetworkConnection {
                    active_path: "/active/1".into(),
                    id: "Skylink".into(),
                    connection_type: "wifi".into(),
                    state: "activated".into(),
                    ..crate::network::provider::NetworkConnection::default()
                }],
                ..NetworkSnapshot::default()
            },
            prompt: Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                None,
                true,
            )),
            active_action: Some(NetworkActiveAction::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            }),
            scanning: false,
        });
        let mut pending_prompt = Some(PendingPrompt {
            id: NetworkPromptId(1),
            ssid: "Skylink".into(),
            path: "/ap/1".into(),
            submitting: true,
            active_connection_path: Some("/active/1".into()),
            connection_uuid: None,
            settings_path: Some("/settings/1".into()),
        });

        let reconciliation = reconcile_pending_prompt(&state_tx, &mut pending_prompt);

        assert_eq!(
            reconciliation,
            Some(PendingPromptReconciliation::Save {
                settings_path: "/settings/1".into(),
            })
        );
        assert_eq!(state_tx.borrow().prompt, None);
        assert_eq!(pending_prompt, None);
    }

    #[test]
    fn restoring_retryable_prompt_clears_pending_submission_state() {
        let (state_tx, _state_rx) = watch::channel(NetworkServiceState {
            health: NetworkServiceHealth::Ready,
            snapshot: NetworkSnapshot::default(),
            prompt: Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                None,
                true,
            )),
            active_action: Some(NetworkActiveAction::ConnectWifi {
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
            }),
            scanning: false,
        });
        let mut pending = PendingPrompt {
            id: NetworkPromptId(1),
            ssid: "Skylink".into(),
            path: "/ap/1".into(),
            submitting: true,
            active_connection_path: Some("/active/1".into()),
            connection_uuid: Some("uuid-1".into()),
            settings_path: Some("/settings/1".into()),
        };

        restore_pending_prompt_after_submit_error(
            &state_tx,
            &mut pending,
            "Failed to start connection. Try again.",
        );

        let state = state_tx.borrow().clone();
        assert_eq!(
            state.prompt,
            Some(network_password_prompt(
                NetworkPromptId(1),
                "Skylink".into(),
                Some("Failed to start connection. Try again.".into()),
                false,
            ))
        );
        assert_eq!(state.active_action, None);
        assert_eq!(
            pending,
            PendingPrompt {
                id: NetworkPromptId(1),
                ssid: "Skylink".into(),
                path: "/ap/1".into(),
                submitting: false,
                active_connection_path: None,
                connection_uuid: None,
                settings_path: None,
            }
        );
    }
}
