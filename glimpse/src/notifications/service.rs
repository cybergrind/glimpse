use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use anyhow::{Result, anyhow};
use tokio::sync::{Notify, broadcast, mpsc, oneshot, watch};
use tracing::warn;

use crate::notifications::persistence::{
    load_notifications_dnd, load_notifications_dnd_from, notifications_state_path,
    save_notifications_dnd, save_notifications_dnd_to,
};
use crate::notifications::protocol::{
    NotificationEntry, NotificationsActiveAction, NotificationsCommand, NotificationsServiceHealth,
    NotificationsServiceState,
};
use crate::notifications::server::{emit_signal, register_server, unregister_server};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationsSignal {
    NotificationClosed { id: u32, reason: u32 },
    ActionInvoked { id: u32, action_key: String },
    ActivationToken { id: u32, token: String },
}

enum ServiceRequest {
    Inject {
        entry: NotificationEntry,
        reply: oneshot::Sender<Result<()>>,
    },
    CloseFromServer {
        id: u32,
        reply: oneshot::Sender<Result<()>>,
    },
    Command {
        command: NotificationsCommand,
        reply: oneshot::Sender<Result<()>>,
    },
}

#[derive(Clone)]
pub(crate) struct NotificationsServerDispatcher {
    requests: mpsc::Sender<ServiceRequest>,
}

impl NotificationsServerDispatcher {
    fn new(requests: mpsc::Sender<ServiceRequest>) -> Self {
        Self { requests }
    }

    pub(crate) async fn inject(&self, entry: NotificationEntry) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.requests
            .send(ServiceRequest::Inject {
                entry,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow!("notifications service stopped"))?;
        reply_rx
            .await
            .map_err(|_| anyhow!("notifications service reply dropped"))?
    }

    pub(crate) async fn close_from_server(&self, id: u32) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.requests
            .send(ServiceRequest::CloseFromServer {
                id,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow!("notifications service stopped"))?;
        reply_rx
            .await
            .map_err(|_| anyhow!("notifications service reply dropped"))?
    }
}

struct HandleLifecycle {
    active_handles: AtomicUsize,
    shutdown_notify: Arc<Notify>,
}

impl HandleLifecycle {
    fn new(shutdown_notify: Arc<Notify>) -> Self {
        Self {
            active_handles: AtomicUsize::new(1),
            shutdown_notify,
        }
    }

    fn clone_handle(&self) {
        self.active_handles.fetch_add(1, Ordering::AcqRel);
    }

    fn drop_handle(&self) {
        if self.active_handles.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.shutdown_notify.notify_waiters();
        }
    }
}

pub struct NotificationsServiceHandle {
    requests: mpsc::Sender<ServiceRequest>,
    state: watch::Receiver<NotificationsServiceState>,
    test_signals: Option<broadcast::Sender<NotificationsSignal>>,
    lifecycle: Arc<HandleLifecycle>,
}

impl Clone for NotificationsServiceHandle {
    fn clone(&self) -> Self {
        self.lifecycle.clone_handle();
        Self {
            requests: self.requests.clone(),
            state: self.state.clone(),
            test_signals: self.test_signals.clone(),
            lifecycle: self.lifecycle.clone(),
        }
    }
}

impl Drop for NotificationsServiceHandle {
    fn drop(&mut self) {
        self.lifecycle.drop_handle();
    }
}

impl NotificationsServiceHandle {
    pub fn new(session: zbus::Connection) -> Self {
        Self::spawn(ServiceRuntimeConfig::production(session))
    }

    pub fn new_for_tests() -> Self {
        Self::spawn(ServiceRuntimeConfig::for_tests(None))
    }

    pub fn new_for_tests_with_persistence_path(path: PathBuf) -> Self {
        Self::spawn(ServiceRuntimeConfig::for_tests(Some(path)))
    }

    pub fn new_for_tests_with_signal_failure() -> Self {
        Self::spawn(
            ServiceRuntimeConfig::for_tests(None)
                .with_signal_failure("forced notifications signal emission failure"),
        )
    }

    pub fn new_for_tests_with_shutdown_probe() -> (Self, watch::Receiver<bool>) {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        (
            Self::spawn(ServiceRuntimeConfig::for_tests(None).with_shutdown_probe(shutdown_tx)),
            shutdown_rx,
        )
    }

    pub fn subscribe(&self) -> watch::Receiver<NotificationsServiceState> {
        self.state.clone()
    }

    pub fn subscribe_test_signals(&self) -> broadcast::Receiver<NotificationsSignal> {
        self.test_signals
            .as_ref()
            .expect("test signals are only available on test handles")
            .subscribe()
    }

    pub async fn send(&self, command: NotificationsCommand) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.requests
            .send(ServiceRequest::Command {
                command,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow!("notifications service stopped"))?;
        reply_rx
            .await
            .map_err(|_| anyhow!("notifications service reply dropped"))?
    }

    pub async fn inject(&self, entry: NotificationEntry) -> Result<()> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.requests
            .send(ServiceRequest::Inject {
                entry,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow!("notifications service stopped"))?;
        reply_rx
            .await
            .map_err(|_| anyhow!("notifications service reply dropped"))?
    }

    fn spawn(config: ServiceRuntimeConfig) -> Self {
        let (requests, request_rx) = mpsc::channel(256);
        let (state_tx, state) = watch::channel(NotificationsServiceState::default());
        let shutdown_notify = Arc::new(Notify::new());
        let lifecycle = Arc::new(HandleLifecycle::new(shutdown_notify.clone()));
        let runtime = ServiceRuntime::new(config, Arc::downgrade(&lifecycle), shutdown_notify);
        let test_signals = runtime.signal_events.clone();
        let dispatcher = NotificationsServerDispatcher::new(requests.clone());

        tokio::spawn(async move {
            NotificationsService::new(runtime, dispatcher, state_tx, request_rx)
                .run()
                .await;
        });

        Self {
            requests,
            state,
            test_signals,
            lifecycle,
        }
    }
}

struct ServiceRuntimeConfig {
    session: Option<zbus::Connection>,
    persistence_path: Option<PathBuf>,
    signal_events: Option<broadcast::Sender<NotificationsSignal>>,
    forced_signal_failure: Option<String>,
    shutdown_probe: Option<watch::Sender<bool>>,
}

impl ServiceRuntimeConfig {
    fn production(session: zbus::Connection) -> Self {
        Self {
            session: Some(session),
            persistence_path: notifications_state_path(),
            signal_events: None,
            forced_signal_failure: None,
            shutdown_probe: None,
        }
    }

    fn for_tests(persistence_path: Option<PathBuf>) -> Self {
        let (signal_events, _) = broadcast::channel(32);
        Self {
            session: None,
            persistence_path,
            signal_events: Some(signal_events),
            forced_signal_failure: None,
            shutdown_probe: None,
        }
    }

    fn with_signal_failure(mut self, message: impl Into<String>) -> Self {
        self.forced_signal_failure = Some(message.into());
        self
    }

    fn with_shutdown_probe(mut self, probe: watch::Sender<bool>) -> Self {
        self.shutdown_probe = Some(probe);
        self
    }
}

struct ServiceRuntime {
    session: Option<zbus::Connection>,
    persistence_path: Option<PathBuf>,
    signal_events: Option<broadcast::Sender<NotificationsSignal>>,
    forced_signal_failure: Option<String>,
    handle_lifecycle: Weak<HandleLifecycle>,
    shutdown_notify: Arc<Notify>,
    shutdown_probe: Option<watch::Sender<bool>>,
}

impl ServiceRuntime {
    fn new(
        config: ServiceRuntimeConfig,
        handle_lifecycle: Weak<HandleLifecycle>,
        shutdown_notify: Arc<Notify>,
    ) -> Self {
        Self {
            session: config.session,
            persistence_path: config.persistence_path,
            signal_events: config.signal_events,
            forced_signal_failure: config.forced_signal_failure,
            handle_lifecycle,
            shutdown_notify,
            shutdown_probe: config.shutdown_probe,
        }
    }

    fn has_external_handles(&self) -> bool {
        self.handle_lifecycle
            .upgrade()
            .is_some_and(|lifecycle| lifecycle.active_handles.load(Ordering::Acquire) > 0)
    }

    fn load_dnd(&self) -> bool {
        match &self.persistence_path {
            Some(path) => load_notifications_dnd_from(path),
            None if self.session.is_some() => load_notifications_dnd(),
            None => false,
        }
    }

    fn save_dnd(&self, enabled: bool) -> Result<()> {
        match &self.persistence_path {
            Some(path) => save_notifications_dnd_to(path, enabled).map_err(Into::into),
            None if self.session.is_some() => save_notifications_dnd(enabled).map_err(Into::into),
            None => Ok(()),
        }
    }

    fn finish_shutdown(&mut self) {
        if let Some(probe) = self.shutdown_probe.take() {
            let _ = probe.send(true);
        }
    }
}

struct NotificationsService {
    runtime: ServiceRuntime,
    dispatcher: NotificationsServerDispatcher,
    state_tx: watch::Sender<NotificationsServiceState>,
    request_rx: mpsc::Receiver<ServiceRequest>,
}

impl NotificationsService {
    fn new(
        runtime: ServiceRuntime,
        dispatcher: NotificationsServerDispatcher,
        state_tx: watch::Sender<NotificationsServiceState>,
        request_rx: mpsc::Receiver<ServiceRequest>,
    ) -> Self {
        Self {
            runtime,
            dispatcher,
            state_tx,
            request_rx,
        }
    }

    async fn run(mut self) {
        let (base_health, signal_emitter) = self.register_server().await;
        let mut state = NotificationsServiceState {
            health: base_health.clone(),
            notifications: Vec::new(),
            dnd: self.runtime.load_dnd(),
            active_action: None,
        };
        let _ = self.state_tx.send(state.clone());

        loop {
            if !self.runtime.has_external_handles() {
                break;
            }

            tokio::select! {
                maybe_request = self.request_rx.recv() => {
                    let Some(request) = maybe_request else {
                        break;
                    };

                    let (result, signal_issues, reply) = self
                        .handle_request(&mut state, signal_emitter.as_ref(), request)
                        .await;
                    update_health_from_outcome(&mut state, &base_health, &result, &signal_issues);
                    let _ = reply.send(clone_result(&result));
                    let _ = self.state_tx.send(state.clone());
                }
                _ = self.runtime.shutdown_notify.notified() => {
                    if !self.runtime.has_external_handles() {
                        break;
                    }
                }
            }
        }

        if let Some(session) = &self.runtime.session {
            unregister_server(session).await;
        }
        self.runtime.finish_shutdown();
    }

    async fn register_server(
        &self,
    ) -> (
        NotificationsServiceHealth,
        Option<zbus::object_server::SignalEmitter<'static>>,
    ) {
        let Some(session) = &self.runtime.session else {
            return (NotificationsServiceHealth::Ready, None);
        };

        match register_server(session, self.dispatcher.clone()).await {
            Ok(signal_emitter) => (NotificationsServiceHealth::Ready, Some(signal_emitter)),
            Err(error) => (
                NotificationsServiceHealth::Degraded {
                    message: format!("failed to register notifications server: {error}"),
                },
                None,
            ),
        }
    }

    async fn handle_request(
        &self,
        state: &mut NotificationsServiceState,
        signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
        request: ServiceRequest,
    ) -> (Result<()>, Vec<String>, oneshot::Sender<Result<()>>) {
        match request {
            ServiceRequest::Inject { entry, reply } => {
                upsert_notification(&mut state.notifications, entry);
                (Ok(()), Vec::new(), reply)
            }
            ServiceRequest::CloseFromServer { id, reply } => {
                let mut signal_issues = Vec::new();
                close_notification(
                    self,
                    signal_emitter,
                    &mut state.notifications,
                    id,
                    3,
                    &mut signal_issues,
                )
                .await;
                (Ok(()), signal_issues, reply)
            }
            ServiceRequest::Command { command, reply } => {
                state.active_action = Some(active_action_for(&command));
                let _ = self.state_tx.send(state.clone());

                let (result, signal_issues) =
                    self.handle_command(state, signal_emitter, command).await;
                state.active_action = None;
                (result, signal_issues, reply)
            }
        }
    }

    async fn handle_command(
        &self,
        state: &mut NotificationsServiceState,
        signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
        command: NotificationsCommand,
    ) -> (Result<()>, Vec<String>) {
        let mut signal_issues = Vec::new();
        let result = match command {
            NotificationsCommand::Dismiss { id } => {
                close_notification(
                    self,
                    signal_emitter,
                    &mut state.notifications,
                    id,
                    2,
                    &mut signal_issues,
                )
                .await;
                Ok(())
            }
            NotificationsCommand::DismissAll => {
                let ids: Vec<u32> = state.notifications.iter().map(|entry| entry.id).collect();
                for id in ids {
                    close_notification(
                        self,
                        signal_emitter,
                        &mut state.notifications,
                        id,
                        2,
                        &mut signal_issues,
                    )
                    .await;
                }
                Ok(())
            }
            NotificationsCommand::InvokeAction {
                id,
                action_key,
                activation_token,
            } => {
                if let Some(token) = activation_token {
                    self.emit_signal_best_effort(
                        signal_emitter,
                        NotificationsSignal::ActivationToken { id, token },
                        &mut signal_issues,
                    )
                    .await;
                }
                self.emit_signal_best_effort(
                    signal_emitter,
                    NotificationsSignal::ActionInvoked {
                        id,
                        action_key: action_key.clone(),
                    },
                    &mut signal_issues,
                )
                .await;

                let resident = state
                    .notifications
                    .iter()
                    .find(|entry| entry.id == id)
                    .map(|entry| entry.resident)
                    .unwrap_or(false);
                if resident {
                    Ok(())
                } else {
                    close_notification(
                        self,
                        signal_emitter,
                        &mut state.notifications,
                        id,
                        2,
                        &mut signal_issues,
                    )
                    .await;
                    Ok(())
                }
            }
            NotificationsCommand::SetDnd(enabled) => {
                state.dnd = enabled;
                self.runtime.save_dnd(enabled)
            }
        };

        (result, signal_issues)
    }

    async fn emit_signal_best_effort(
        &self,
        signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
        signal: NotificationsSignal,
        signal_issues: &mut Vec<String>,
    ) {
        if let Some(events) = &self.runtime.signal_events {
            let _ = events.send(signal.clone());
        }

        let emission_result = if let Some(message) = &self.runtime.forced_signal_failure {
            Err(anyhow!(message.clone()))
        } else if let Some(signal_emitter) = signal_emitter {
            emit_signal(signal_emitter, &signal)
                .await
                .map_err(Into::into)
        } else {
            Ok(())
        };

        if let Err(error) = emission_result {
            let issue = format!("failed to emit {} signal: {error}", signal_name(&signal));
            warn!("{issue}");
            signal_issues.push(issue);
        }
    }
}

fn upsert_notification(notifications: &mut Vec<NotificationEntry>, entry: NotificationEntry) {
    if let Some(index) = notifications
        .iter()
        .position(|current| current.id == entry.id)
    {
        notifications[index] = entry;
        return;
    }

    notifications.insert(0, entry);
}

async fn close_notification(
    service: &NotificationsService,
    signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
    notifications: &mut Vec<NotificationEntry>,
    id: u32,
    reason: u32,
    signal_issues: &mut Vec<String>,
) {
    let Some(index) = notifications.iter().position(|entry| entry.id == id) else {
        return;
    };
    notifications.remove(index);

    service
        .emit_signal_best_effort(
            signal_emitter,
            NotificationsSignal::NotificationClosed { id, reason },
            signal_issues,
        )
        .await;
}

fn active_action_for(command: &NotificationsCommand) -> NotificationsActiveAction {
    match command {
        NotificationsCommand::Dismiss { id } => NotificationsActiveAction::Dismiss { id: *id },
        NotificationsCommand::DismissAll => NotificationsActiveAction::DismissAll,
        NotificationsCommand::InvokeAction { id, action_key, .. } => {
            NotificationsActiveAction::InvokeAction {
                id: *id,
                action_key: action_key.clone(),
            }
        }
        NotificationsCommand::SetDnd(enabled) => NotificationsActiveAction::SetDnd(*enabled),
    }
}

fn signal_name(signal: &NotificationsSignal) -> &'static str {
    match signal {
        NotificationsSignal::NotificationClosed { .. } => "NotificationClosed",
        NotificationsSignal::ActionInvoked { .. } => "ActionInvoked",
        NotificationsSignal::ActivationToken { .. } => "ActivationToken",
    }
}

fn clone_result(result: &Result<()>) -> Result<()> {
    result
        .as_ref()
        .map(|_| ())
        .map_err(|error| anyhow!(error.to_string()))
}

fn update_health_from_outcome(
    state: &mut NotificationsServiceState,
    base_health: &NotificationsServiceHealth,
    result: &Result<()>,
    signal_issues: &[String],
) {
    if let Err(error) = result {
        state.health = NotificationsServiceHealth::Degraded {
            message: error.to_string(),
        };
        return;
    }

    if signal_issues.is_empty() {
        state.health = base_health.clone();
    } else {
        state.health = NotificationsServiceHealth::Degraded {
            message: signal_issues.join("; "),
        };
    }
}
