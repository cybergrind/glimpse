use std::{
    collections::{HashMap, HashSet},
    process::ExitStatus,
    sync::Arc,
};

use async_trait::async_trait;
use serde::Serialize;
use tokio::{
    process::Command as TokioCommand,
    sync::{mpsc, watch},
};
use tokio_util::sync::CancellationToken;

use crate::{
    IdleConfig, IdleListenerConfig,
    services::{
        battery,
        framework::{Control, ServiceCommand, ServiceHandle},
    },
};

const COMMAND_QUEUE_SIZE: usize = 16;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PowerSource {
    #[default]
    Ac,
    Battery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Health {
    Disabled,
    Ready,
    Degraded { message: String },
}

impl Default for Health {
    fn default() -> Self {
        Self::Ready
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActiveListener {
    pub id: usize,
    pub timeout: u64,
    pub on_idle: String,
    pub on_resume: String,
    pub respect_inhibitors: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct State {
    pub health: Health,
    pub enabled: bool,
    pub power_source: PowerSource,
    pub listeners: Vec<ActiveListener>,
    pub fired_listeners: Vec<usize>,
    pub generation: u64,
}

impl Default for State {
    fn default() -> Self {
        let config = IdleConfig::default();
        Self {
            health: Health::Ready,
            enabled: config.enabled,
            power_source: PowerSource::Ac,
            listeners: resolve_listeners(&config, PowerSource::Ac),
            fired_listeners: vec![],
            generation: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    ApplyConfig(IdleConfig),
    ListenerIdle(usize),
    ListenerResume(usize),
    SetBackendHealth(Health),
}

pub type IdleHandle = ServiceHandle<State, Command>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    pub success: bool,
    pub code: Option<i32>,
}

impl CommandOutcome {
    fn from_exit_status(status: ExitStatus) -> Self {
        Self {
            success: status.success(),
            code: status.code(),
        }
    }
}

#[async_trait]
pub trait IdleCommandRunner: Send + Sync {
    async fn run(&self, command: &str) -> anyhow::Result<CommandOutcome>;
}

pub struct ShellCommandRunner;

#[async_trait]
impl IdleCommandRunner for ShellCommandRunner {
    async fn run(&self, command: &str) -> anyhow::Result<CommandOutcome> {
        let status = TokioCommand::new("/bin/sh")
            .arg("-c")
            .arg(command)
            .status()
            .await?;
        Ok(CommandOutcome::from_exit_status(status))
    }
}

pub struct IdleService {
    battery: battery::BatteryHandle,
    runner: Arc<dyn IdleCommandRunner>,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    config: IdleConfig,
    fired_listeners: HashSet<usize>,
    command_locks: HashMap<usize, Arc<tokio::sync::Mutex<()>>>,
}

impl IdleService {
    pub fn new(battery: battery::BatteryHandle) -> (Self, IdleHandle) {
        Self::with_runner(battery, Arc::new(ShellCommandRunner))
    }

    pub fn with_runner(
        battery: battery::BatteryHandle,
        runner: Arc<dyn IdleCommandRunner>,
    ) -> (Self, IdleHandle) {
        let config = IdleConfig::default();
        let state = state_for_config(&config, power_source_from_battery(&battery.snapshot()), 0);
        let command_locks = command_locks_for_state(&state);
        let (state_tx, state_rx) = watch::channel(state);
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                battery,
                runner,
                state_tx,
                command_rx,
                config,
                fired_listeners: HashSet::new(),
                command_locks,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        tracing::debug!("idle service started");
        let mut battery_rx = self.battery.subscribe();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = battery_rx.changed() => {
                    if changed.is_err() {
                        self.set_health(Health::Degraded { message: "battery service subscription closed".into() });
                        break;
                    }
                    self.refresh_profile_from_battery();
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Start(config)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(config))) => {
                        self.apply_config(config.idle);
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => break,
                    Some(ServiceCommand::Command(Command::ApplyConfig(config))) => {
                        self.apply_config(config);
                    }
                    Some(ServiceCommand::Command(Command::ListenerIdle(id))) => {
                        self.listener_idle(id).await;
                    }
                    Some(ServiceCommand::Command(Command::ListenerResume(id))) => {
                        self.listener_resume(id).await;
                    }
                    Some(ServiceCommand::Command(Command::SetBackendHealth(health))) => {
                        self.set_health(health);
                    }
                }
            }
        }

        tracing::debug!("idle service quit");
    }

    fn apply_config(&mut self, config: IdleConfig) {
        if self.config == config {
            return;
        }
        self.config = config;
        self.publish_profile(true);
    }

    fn refresh_profile_from_battery(&mut self) {
        let current = self.state_tx.borrow().power_source;
        let next = power_source_from_battery(&self.battery.snapshot());
        if current != next {
            self.publish_profile(true);
        }
    }

    async fn listener_idle(&mut self, id: usize) {
        let Some(listener) = self.active_listener(id) else {
            tracing::debug!(
                listener = id,
                "idle listener event ignored because listener is inactive"
            );
            return;
        };
        if self.fired_listeners.contains(&id) {
            tracing::debug!(
                listener = id,
                "idle listener event ignored because listener already fired"
            );
            return;
        }

        self.fired_listeners.insert(id);
        self.publish_fired_listeners();
        self.spawn_listener_command("idle", id, listener.timeout, listener.on_idle);
    }

    async fn listener_resume(&mut self, id: usize) {
        let Some(listener) = self.active_listener(id) else {
            tracing::debug!(
                listener = id,
                "resume listener event ignored because listener is inactive"
            );
            return;
        };
        if !self.fired_listeners.remove(&id) {
            tracing::debug!(
                listener = id,
                "resume listener event ignored because listener did not fire"
            );
            return;
        }

        self.publish_fired_listeners();
        self.spawn_listener_command("resume", id, listener.timeout, listener.on_resume);
    }

    fn spawn_listener_command(&self, hook: &'static str, id: usize, timeout: u64, command: String) {
        let Some(lock) = self.command_locks.get(&id).cloned() else {
            tracing::debug!(
                listener = id,
                hook,
                timeout,
                "idle listener command ignored because listener has no command queue"
            );
            return;
        };
        let command = command.trim().to_string();
        if command.is_empty() {
            tracing::debug!(listener = id, hook, timeout, "idle listener has no command");
            return;
        }

        tracing::info!(
            listener = id,
            hook,
            timeout,
            command,
            "running idle listener command"
        );

        let runner = self.runner.clone();
        tokio::spawn(async move {
            let _guard = lock.lock().await;
            match runner.run(&command).await {
                Ok(outcome) if outcome.success => {
                    tracing::info!(
                        listener = id,
                        hook,
                        timeout,
                        code = outcome.code,
                        "idle listener command completed"
                    );
                }
                Ok(outcome) => {
                    tracing::warn!(
                        listener = id,
                        hook,
                        timeout,
                        code = outcome.code,
                        "idle listener command failed"
                    );
                }
                Err(error) => {
                    tracing::warn!(listener = id, hook, timeout, %error, "idle listener command failed to start");
                }
            }
        });
    }

    fn active_listener(&self, id: usize) -> Option<ActiveListener> {
        self.state_tx
            .borrow()
            .listeners
            .iter()
            .find(|listener| listener.id == id)
            .cloned()
    }

    fn publish_profile(&mut self, reset_fired: bool) {
        if reset_fired {
            self.resume_fired_listeners_before_policy_replace();
            self.fired_listeners.clear();
        }
        let current = self.state_tx.borrow().clone();
        let mut state = state_for_config(
            &self.config,
            power_source_from_battery(&self.battery.snapshot()),
            current.generation + 1,
        );
        self.sync_command_locks(&state);
        if state.enabled && current.enabled {
            state.health = current.health;
        }
        state.fired_listeners = sorted_ids(&self.fired_listeners);
        self.state_tx.send_if_modified(|current| {
            if *current == state {
                false
            } else {
                tracing::info!(
                    power_source = ?state.power_source,
                    listeners = state.listeners.len(),
                    generation = state.generation,
                    "idle policy changed"
                );
                *current = state;
                true
            }
        });
    }

    fn publish_fired_listeners(&self) {
        let fired = sorted_ids(&self.fired_listeners);
        self.state_tx.send_if_modified(|state| {
            if state.fired_listeners == fired {
                false
            } else {
                state.fired_listeners = fired;
                true
            }
        });
    }

    fn set_health(&self, health: Health) {
        self.state_tx.send_if_modified(|state| {
            let health = if state.enabled {
                health.clone()
            } else {
                Health::Disabled
            };
            if state.health == health {
                false
            } else {
                state.health = health;
                true
            }
        });
    }

    fn resume_fired_listeners_before_policy_replace(&mut self) {
        for id in sorted_ids(&self.fired_listeners) {
            let Some(listener) = self.active_listener(id) else {
                continue;
            };
            self.spawn_listener_command(
                "resume-policy-replace",
                id,
                listener.timeout,
                listener.on_resume,
            );
        }
    }

    fn sync_command_locks(&mut self, state: &State) {
        self.command_locks
            .retain(|id, _| state.listeners.iter().any(|listener| listener.id == *id));
        for listener in &state.listeners {
            self.command_locks
                .entry(listener.id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())));
        }
    }
}

fn command_locks_for_state(state: &State) -> HashMap<usize, Arc<tokio::sync::Mutex<()>>> {
    state
        .listeners
        .iter()
        .map(|listener| (listener.id, Arc::new(tokio::sync::Mutex::new(()))))
        .collect()
}

fn state_for_config(config: &IdleConfig, power_source: PowerSource, generation: u64) -> State {
    State {
        health: if config.enabled {
            Health::Ready
        } else {
            Health::Disabled
        },
        enabled: config.enabled,
        power_source,
        listeners: if config.enabled {
            resolve_listeners(config, power_source)
        } else {
            vec![]
        },
        fired_listeners: vec![],
        generation,
    }
}

fn resolve_listeners(config: &IdleConfig, power_source: PowerSource) -> Vec<ActiveListener> {
    let listeners = match power_source {
        PowerSource::Ac => &config.profiles.ac.listeners,
        PowerSource::Battery => &config.profiles.battery.listeners,
    };

    listeners
        .iter()
        .enumerate()
        .filter(|(_, listener)| listener.timeout > 0)
        .map(|(id, listener)| active_listener(id, listener, config.respect_inhibitors))
        .collect()
}

fn active_listener(
    id: usize,
    listener: &IdleListenerConfig,
    global_respect_inhibitors: bool,
) -> ActiveListener {
    ActiveListener {
        id,
        timeout: listener.timeout,
        on_idle: listener.on_idle.clone(),
        on_resume: listener.on_resume.clone(),
        respect_inhibitors: listener
            .respect_inhibitors
            .unwrap_or(global_respect_inhibitors),
    }
}

fn power_source_from_battery(state: &battery::State) -> PowerSource {
    if state.status.on_battery {
        PowerSource::Battery
    } else {
        PowerSource::Ac
    }
}

fn sorted_ids(ids: &HashSet<usize>) -> Vec<usize> {
    let mut ids = ids.iter().copied().collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{Command, CommandOutcome, Health, IdleCommandRunner, IdleService, PowerSource};
    use crate::{
        Config, IdleConfig, IdleListenerConfig, IdleProfileConfig, IdleProfilesConfig,
        services::{
            battery,
            framework::{ServiceCommand, ServiceHandle},
        },
    };
    use async_trait::async_trait;
    use tokio::sync::{mpsc, watch};
    use tokio_util::sync::CancellationToken;

    #[derive(Clone, Default)]
    struct RecordedCommands(Arc<Mutex<Vec<String>>>);

    struct RecordingRunner {
        commands: RecordedCommands,
    }

    #[async_trait]
    impl IdleCommandRunner for RecordingRunner {
        async fn run(&self, command: &str) -> anyhow::Result<CommandOutcome> {
            self.commands.0.lock().unwrap().push(command.to_string());
            Ok(CommandOutcome {
                success: true,
                code: Some(0),
            })
        }
    }

    struct BlockingRunner {
        started: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    #[async_trait]
    impl IdleCommandRunner for BlockingRunner {
        async fn run(&self, _command: &str) -> anyhow::Result<CommandOutcome> {
            self.started.notify_waiters();
            self.release.notified().await;
            Ok(CommandOutcome {
                success: true,
                code: Some(0),
            })
        }
    }

    fn battery_handle(on_battery: bool) -> (watch::Sender<battery::State>, battery::BatteryHandle) {
        let state = battery::State {
            status: battery::BatteryStatus {
                on_battery,
                ..battery::BatteryStatus::default()
            },
            ..battery::State::default()
        };
        let (state_tx, state_rx) = watch::channel(state);
        let (command_tx, _command_rx) = mpsc::channel(4);
        (state_tx, ServiceHandle::new(state_rx, command_tx))
    }

    fn test_config() -> IdleConfig {
        IdleConfig {
            enabled: true,
            respect_inhibitors: true,
            profiles: IdleProfilesConfig {
                ac: IdleProfileConfig {
                    listeners: vec![
                        IdleListenerConfig::new(10, "ac-idle", "ac-resume"),
                        IdleListenerConfig {
                            timeout: 0,
                            on_idle: "disabled".into(),
                            on_resume: String::new(),
                            respect_inhibitors: None,
                        },
                    ],
                },
                battery: IdleProfileConfig {
                    listeners: vec![IdleListenerConfig::new(5, "battery-idle", "battery-resume")],
                },
            },
        }
    }

    #[test]
    fn default_state_uses_ac_profile_without_listener_policies() {
        let (_battery_tx, battery) = battery_handle(false);
        let (_service, handle) = IdleService::new(battery);

        let state = handle.snapshot();
        assert_eq!(state.power_source, PowerSource::Ac);
        assert!(state.enabled);
        assert!(state.listeners.is_empty());
    }

    #[test]
    fn configured_listener_resolves_inhibitor_override() {
        let listeners = super::resolve_listeners(&test_config(), PowerSource::Ac);

        assert!(listeners[0].respect_inhibitors);
    }

    #[tokio::test]
    async fn idle_and_resume_run_listener_commands() {
        let (_battery_tx, battery) = battery_handle(false);
        let commands = RecordedCommands::default();
        let (service, handle) = IdleService::with_runner(
            battery,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(task_cancel).await });

        handle
            .send(ServiceCommand::Command(Command::ApplyConfig(test_config())))
            .await
            .unwrap();
        handle
            .send(ServiceCommand::Command(Command::ListenerIdle(0)))
            .await
            .unwrap();
        handle
            .send(ServiceCommand::Command(Command::ListenerResume(0)))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        assert_eq!(
            *commands.0.lock().unwrap(),
            vec!["ac-idle".to_string(), "ac-resume".to_string()]
        );
    }

    #[tokio::test]
    async fn resume_without_fired_listener_is_ignored() {
        let (_battery_tx, battery) = battery_handle(false);
        let commands = RecordedCommands::default();
        let (service, handle) = IdleService::with_runner(
            battery,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(task_cancel).await });

        handle
            .send(ServiceCommand::Command(Command::ApplyConfig(test_config())))
            .await
            .unwrap();
        handle
            .send(ServiceCommand::Command(Command::ListenerResume(0)))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        assert!(commands.0.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn long_running_command_does_not_block_resume_state() {
        let (_battery_tx, battery) = battery_handle(false);
        let runner = Arc::new(BlockingRunner {
            started: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        let (service, handle) = IdleService::with_runner(battery, runner.clone());
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(task_cancel).await });

        handle
            .send(ServiceCommand::Command(Command::ApplyConfig(test_config())))
            .await
            .unwrap();
        let started = runner.started.notified();
        handle
            .send(ServiceCommand::Command(Command::ListenerIdle(0)))
            .await
            .unwrap();
        started.await;
        handle
            .send(ServiceCommand::Command(Command::ListenerResume(0)))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;

        assert!(handle.snapshot().fired_listeners.is_empty());

        runner.release.notify_waiters();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;
    }

    #[tokio::test]
    async fn battery_change_switches_active_profile_and_clears_fired_state() {
        let (battery_tx, battery) = battery_handle(false);
        let commands = RecordedCommands::default();
        let (service, handle) = IdleService::with_runner(
            battery,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(task_cancel).await });

        handle
            .send(ServiceCommand::Command(Command::ApplyConfig(test_config())))
            .await
            .unwrap();
        handle
            .send(ServiceCommand::Command(Command::ListenerIdle(0)))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        let mut next = battery_tx.borrow().clone();
        next.status.on_battery = true;
        battery_tx.send(next).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        let state = handle.snapshot();
        assert_eq!(state.power_source, PowerSource::Battery);
        assert_eq!(state.listeners[0].timeout, 5);
        assert!(state.fired_listeners.is_empty());
        assert_eq!(
            *commands.0.lock().unwrap(),
            vec!["ac-idle".to_string(), "ac-resume".to_string()]
        );
    }

    #[tokio::test]
    async fn config_change_runs_pending_resume_before_replacing_policy() {
        let (_battery_tx, battery) = battery_handle(false);
        let commands = RecordedCommands::default();
        let (service, handle) = IdleService::with_runner(
            battery,
            Arc::new(RecordingRunner {
                commands: commands.clone(),
            }),
        );
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(task_cancel).await });
        let mut next_config = test_config();
        next_config.profiles.ac.listeners[0].on_resume = "next-resume".into();

        handle
            .send(ServiceCommand::Command(Command::ApplyConfig(test_config())))
            .await
            .unwrap();
        handle
            .send(ServiceCommand::Command(Command::ListenerIdle(0)))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        handle
            .send(ServiceCommand::Command(Command::ApplyConfig(next_config)))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        assert_eq!(
            *commands.0.lock().unwrap(),
            vec!["ac-idle".to_string(), "ac-resume".to_string()]
        );
        assert!(handle.snapshot().fired_listeners.is_empty());
    }

    #[tokio::test]
    async fn control_reconfigure_applies_idle_config_from_shared_config() {
        let (_battery_tx, battery) = battery_handle(false);
        let (service, handle) = IdleService::with_runner(
            battery,
            Arc::new(RecordingRunner {
                commands: RecordedCommands::default(),
            }),
        );
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(task_cancel).await });
        let config = Config {
            idle: test_config(),
            ..Config::default()
        };

        handle
            .send(ServiceCommand::Control(
                crate::services::framework::Control::Reconfigure(config),
            ))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        assert_eq!(handle.snapshot().listeners[0].timeout, 10);
    }

    #[tokio::test]
    async fn disabled_config_keeps_disabled_health_when_backend_reports_ready() {
        let (_battery_tx, battery) = battery_handle(false);
        let (service, handle) = IdleService::with_runner(
            battery,
            Arc::new(RecordingRunner {
                commands: RecordedCommands::default(),
            }),
        );
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(task_cancel).await });
        let config = IdleConfig {
            enabled: false,
            ..IdleConfig::default()
        };

        handle
            .send(ServiceCommand::Command(Command::ApplyConfig(config)))
            .await
            .unwrap();
        handle
            .send(ServiceCommand::Command(Command::SetBackendHealth(
                Health::Ready,
            )))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        assert_eq!(handle.snapshot().health, Health::Disabled);
    }

    #[tokio::test]
    async fn profile_change_preserves_degraded_backend_health() {
        let (battery_tx, battery) = battery_handle(false);
        let (service, handle) = IdleService::with_runner(
            battery,
            Arc::new(RecordingRunner {
                commands: RecordedCommands::default(),
            }),
        );
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(task_cancel).await });

        handle
            .send(ServiceCommand::Command(Command::SetBackendHealth(
                Health::Degraded {
                    message: "backend unavailable".into(),
                },
            )))
            .await
            .unwrap();
        let mut next = battery_tx.borrow().clone();
        next.status.on_battery = true;
        battery_tx.send(next).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        assert_eq!(
            handle.snapshot().health,
            Health::Degraded {
                message: "backend unavailable".into()
            }
        );
    }
}
