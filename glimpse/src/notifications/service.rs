use std::path::PathBuf;

use anyhow::{Result, anyhow};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

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

#[derive(Clone)]
pub struct NotificationsServiceHandle {
    requests: mpsc::Sender<ServiceRequest>,
    state: watch::Receiver<NotificationsServiceState>,
    test_signals: Option<broadcast::Sender<NotificationsSignal>>,
}

impl NotificationsServiceHandle {
    pub fn new(session: zbus::Connection) -> Self {
        Self::spawn(ServiceRuntime::production(session))
    }

    pub fn new_for_tests() -> Self {
        Self::spawn(ServiceRuntime::for_tests(None))
    }

    pub fn new_for_tests_with_persistence_path(path: PathBuf) -> Self {
        Self::spawn(ServiceRuntime::for_tests(Some(path)))
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

    fn spawn(runtime: ServiceRuntime) -> Self {
        let (requests, request_rx) = mpsc::channel(256);
        let (state_tx, state) = watch::channel(NotificationsServiceState::default());
        let test_signals = runtime.signal_events.clone();
        let handle = Self {
            requests: requests.clone(),
            state: state.clone(),
            test_signals: test_signals.clone(),
        };

        tokio::spawn(async move {
            NotificationsService::new(runtime, handle, state_tx, request_rx)
                .run()
                .await;
        });

        Self {
            requests,
            state,
            test_signals,
        }
    }
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
struct ServiceRuntime {
    session: Option<zbus::Connection>,
    persistence_path: Option<PathBuf>,
    signal_events: Option<broadcast::Sender<NotificationsSignal>>,
}

impl ServiceRuntime {
    fn production(session: zbus::Connection) -> Self {
        Self {
            session: Some(session),
            persistence_path: notifications_state_path(),
            signal_events: None,
        }
    }

    fn for_tests(persistence_path: Option<PathBuf>) -> Self {
        let (signal_events, _) = broadcast::channel(32);
        Self {
            session: None,
            persistence_path,
            signal_events: Some(signal_events),
        }
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
            None => save_notifications_dnd(enabled).map_err(Into::into),
        }
    }
}

struct NotificationsService {
    runtime: ServiceRuntime,
    handle: NotificationsServiceHandle,
    state_tx: watch::Sender<NotificationsServiceState>,
    request_rx: mpsc::Receiver<ServiceRequest>,
}

impl NotificationsService {
    fn new(
        runtime: ServiceRuntime,
        handle: NotificationsServiceHandle,
        state_tx: watch::Sender<NotificationsServiceState>,
        request_rx: mpsc::Receiver<ServiceRequest>,
    ) -> Self {
        Self {
            runtime,
            handle,
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

        while let Some(request) = self.request_rx.recv().await {
            match request {
                ServiceRequest::Inject { entry, reply } => {
                    let result = self.handle_inject(&mut state, entry).await;
                    update_health_from_result(&mut state, &result);
                    let _ = reply.send(clone_result(&result));
                }
                ServiceRequest::CloseFromServer { id, reply } => {
                    let result = self
                        .handle_close_from_server(&mut state, signal_emitter.as_ref(), id)
                        .await;
                    update_health_from_result(&mut state, &result);
                    let _ = reply.send(clone_result(&result));
                }
                ServiceRequest::Command { command, reply } => {
                    let result = self
                        .handle_command(&mut state, signal_emitter.as_ref(), &base_health, command)
                        .await;
                    update_health_from_result(&mut state, &result);
                    let _ = reply.send(clone_result(&result));
                }
            }

            let _ = self.state_tx.send(state.clone());
        }

        if let Some(session) = &self.runtime.session {
            unregister_server(session).await;
        }
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

        match register_server(session, self.handle.clone()).await {
            Ok(signal_emitter) => (NotificationsServiceHealth::Ready, Some(signal_emitter)),
            Err(error) => (
                NotificationsServiceHealth::Degraded {
                    message: format!("failed to register notifications server: {error}"),
                },
                None,
            ),
        }
    }

    async fn handle_inject(
        &self,
        state: &mut NotificationsServiceState,
        entry: NotificationEntry,
    ) -> Result<()> {
        upsert_notification(&mut state.notifications, entry);
        state.health = ready_health(&state.health);
        Ok(())
    }

    async fn handle_close_from_server(
        &self,
        state: &mut NotificationsServiceState,
        signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
        id: u32,
    ) -> Result<()> {
        close_notification(
            &self.runtime,
            signal_emitter,
            &mut state.notifications,
            id,
            3,
        )
        .await?;
        Ok(())
    }

    async fn handle_command(
        &self,
        state: &mut NotificationsServiceState,
        signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
        base_health: &NotificationsServiceHealth,
        command: NotificationsCommand,
    ) -> Result<()> {
        state.active_action = Some(active_action_for(&command));
        let _ = self.state_tx.send(state.clone());

        let result = match command {
            NotificationsCommand::Dismiss { id } => {
                close_notification(
                    &self.runtime,
                    signal_emitter,
                    &mut state.notifications,
                    id,
                    2,
                )
                .await
            }
            NotificationsCommand::DismissAll => {
                let ids: Vec<u32> = state.notifications.iter().map(|entry| entry.id).collect();
                for id in ids {
                    close_notification(
                        &self.runtime,
                        signal_emitter,
                        &mut state.notifications,
                        id,
                        2,
                    )
                    .await?;
                }
                Ok(())
            }
            NotificationsCommand::InvokeAction {
                id,
                action_key,
                activation_token,
            } => {
                if let Some(token) = activation_token {
                    self.emit_signal(
                        signal_emitter,
                        NotificationsSignal::ActivationToken { id, token },
                    )
                    .await?;
                }
                self.emit_signal(
                    signal_emitter,
                    NotificationsSignal::ActionInvoked {
                        id,
                        action_key: action_key.clone(),
                    },
                )
                .await?;

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
                        &self.runtime,
                        signal_emitter,
                        &mut state.notifications,
                        id,
                        2,
                    )
                    .await
                }
            }
            NotificationsCommand::SetDnd(enabled) => {
                state.dnd = enabled;
                self.runtime.save_dnd(enabled)?;
                Ok(())
            }
        };

        state.active_action = None;
        if result.is_ok() {
            state.health = base_health.clone();
        }
        result
    }

    async fn emit_signal(
        &self,
        signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
        signal: NotificationsSignal,
    ) -> Result<()> {
        if let Some(events) = &self.runtime.signal_events {
            let _ = events.send(signal.clone());
        }
        if let Some(signal_emitter) = signal_emitter {
            emit_signal(signal_emitter, &signal).await?;
        }
        Ok(())
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
    runtime: &ServiceRuntime,
    signal_emitter: Option<&zbus::object_server::SignalEmitter<'static>>,
    notifications: &mut Vec<NotificationEntry>,
    id: u32,
    reason: u32,
) -> Result<()> {
    let Some(index) = notifications.iter().position(|entry| entry.id == id) else {
        return Ok(());
    };
    notifications.remove(index);

    let signal = NotificationsSignal::NotificationClosed { id, reason };
    if let Some(events) = &runtime.signal_events {
        let _ = events.send(signal.clone());
    }
    if let Some(signal_emitter) = signal_emitter {
        emit_signal(signal_emitter, &signal).await?;
    }
    Ok(())
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

fn ready_health(current: &NotificationsServiceHealth) -> NotificationsServiceHealth {
    match current {
        NotificationsServiceHealth::Degraded { message } => NotificationsServiceHealth::Degraded {
            message: message.clone(),
        },
        _ => NotificationsServiceHealth::Ready,
    }
}

fn clone_result(result: &Result<()>) -> Result<()> {
    result
        .as_ref()
        .map(|_| ())
        .map_err(|error| anyhow!(error.to_string()))
}

fn update_health_from_result(state: &mut NotificationsServiceState, result: &Result<()>) {
    if let Err(error) = result {
        state.health = NotificationsServiceHealth::Degraded {
            message: error.to_string(),
        };
    }
}
