use glimpse_core::{
    Config, ConfigEvent,
    services::{
        battery::{BatteryHandle, BatteryService},
        framework::Control,
        idle::{self, IdleHandle, IdleService, State},
    },
    watch_for_config_changes,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::backend;

struct AppTask {
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

impl AppTask {
    async fn cancel(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }
}

pub async fn run(config: Config) -> anyhow::Result<()> {
    let cancel = CancellationToken::new();
    let mut running_services = Vec::new();
    let system_dbus = zbus::Connection::system().await?;

    let (battery_service, battery) = BatteryService::new(system_dbus);
    running_services.push(spawn_service(cancel.clone(), |cancel| {
        battery_service.run(cancel)
    }));

    let (idle_service, idle) = IdleService::new(battery.clone());
    running_services.push(spawn_service(cancel.clone(), |cancel| {
        idle_service.run(cancel)
    }));

    start_services(&battery, &idle, config.clone());
    running_services.push(spawn_idle_subscription(idle.clone(), cancel.clone()));
    let backend_idle = idle.clone();
    running_services.push(spawn_service(cancel.clone(), move |cancel| {
        backend::run(backend_idle.clone(), cancel)
    }));

    let (config_tx, mut config_rx) = mpsc::channel(1);
    let config_cancel = cancel.clone();
    running_services.push(spawn_service(cancel.clone(), move |_| async move {
        tokio::select! {
            _ = config_cancel.cancelled() => {}
            _ = watch_for_config_changes(config_tx) => {}
        }
    }));

    tracing::info!("glimpse-idle is running");
    let mut current_idle_config = config.idle;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received shutdown signal");
                break;
            }
            message = config_rx.recv() => match message {
                Some(ConfigEvent::Changed(config)) => {
                    if current_idle_config == config.idle {
                        continue;
                    }
                    tracing::info!("idle config changed");
                    reconfigure_services(&idle, config.clone());
                    current_idle_config = config.idle;
                }
                None => break,
            }
        }
    }

    shutdown_services(&battery, &idle);
    cancel.cancel();
    for service in running_services {
        service.cancel().await;
    }
    tracing::info!("glimpse-idle stopped");

    Ok(())
}

pub fn start_services(battery: &BatteryHandle, idle: &IdleHandle, config: Config) {
    battery.try_send_control(
        "battery",
        Control::Start(config.clone()),
        "failed to send service control",
    );
    idle.try_send_control(
        "idle",
        Control::Start(config),
        "failed to send service control",
    );
}

pub fn reconfigure_services(idle: &IdleHandle, config: Config) {
    idle.try_send_command(
        "idle",
        idle::Command::ApplyConfig(config.idle),
        "failed to send idle config update",
    );
}

fn shutdown_services(battery: &BatteryHandle, idle: &IdleHandle) {
    battery.try_send_control(
        "battery",
        Control::Shutdown,
        "failed to send service control",
    );
    idle.try_send_control("idle", Control::Shutdown, "failed to send service control");
}

fn spawn_idle_subscription(idle: IdleHandle, cancel: CancellationToken) -> AppTask {
    spawn_service(cancel.clone(), move |_| async move {
        let mut state_rx = idle.subscribe();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    log_idle_state(&state_rx.borrow().clone());
                }
            }
        }
    })
}

fn log_idle_state(state: &State) {
    let timeouts = state
        .listeners
        .iter()
        .map(|listener| listener.timeout.to_string())
        .collect::<Vec<_>>()
        .join(",");
    tracing::info!(
        enabled = state.enabled,
        health = ?state.health,
        power_source = ?state.power_source,
        generation = state.generation,
        listeners = state.listeners.len(),
        timeouts,
        "idle state changed"
    );
}

fn spawn_service<F, Fut>(cancel: CancellationToken, run: F) -> AppTask
where
    F: FnOnce(CancellationToken) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let task_cancel = cancel.clone();
    let task = tokio::spawn(async move { run(task_cancel).await });
    AppTask { cancel, task }
}

#[cfg(test)]
mod tests {
    use super::{reconfigure_services, start_services};
    use glimpse_core::{
        Config, IdleConfig,
        services::{
            battery,
            framework::{Control, ServiceCommand, ServiceHandle},
            idle,
        },
    };
    use tokio::sync::{mpsc, watch};

    fn handle<State: Clone, Command: Send>(
        state: State,
    ) -> (
        ServiceHandle<State, Command>,
        mpsc::Receiver<ServiceCommand<Command>>,
    ) {
        let (_state_tx, state_rx) = watch::channel(state);
        let (command_tx, command_rx) = mpsc::channel(4);
        (ServiceHandle::new(state_rx, command_tx), command_rx)
    }

    #[tokio::test]
    async fn start_services_sends_start_control_to_battery_and_idle() {
        let (battery, mut battery_rx) =
            handle::<battery::State, battery::Command>(battery::State::default());
        let (idle, mut idle_rx) = handle::<idle::State, idle::Command>(idle::State::default());

        start_services(&battery, &idle, Config::default());

        assert!(matches!(
            battery_rx.recv().await,
            Some(ServiceCommand::Control(Control::Start(_)))
        ));
        assert!(matches!(
            idle_rx.recv().await,
            Some(ServiceCommand::Control(Control::Start(_)))
        ));
    }

    #[tokio::test]
    async fn reconfigure_services_sends_idle_config_command() {
        let (idle, mut idle_rx) = handle::<idle::State, idle::Command>(idle::State::default());
        let config = Config {
            idle: IdleConfig {
                enabled: false,
                ..IdleConfig::default()
            },
            ..Config::default()
        };

        reconfigure_services(&idle, config);

        assert!(matches!(
            idle_rx.recv().await,
            Some(ServiceCommand::Command(idle::Command::ApplyConfig(config))) if !config.enabled
        ));
    }
}
