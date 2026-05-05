use glimpse_core::{
    Config, ConfigEvent,
    services::{
        framework::{Control, ServiceCommand, ServiceHandle},
        location,
        night_light::{self, NightLightHandle, NightLightService, State},
    },
    watch_for_config_changes,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::backend::create_backend;

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

    let (location_service, location) = location::LocationService::new_standalone();
    running_services.push(spawn_service(cancel.clone(), |cancel| {
        location_service.run(cancel)
    }));

    let backend = create_backend(glimpse_core::compositors::detect_compositor());
    let (night_light_service, night_light) = NightLightService::new(backend, location.clone());
    running_services.push(spawn_service(cancel.clone(), |cancel| {
        night_light_service.run(cancel)
    }));

    start_services(&location, &night_light, config.clone());
    running_services.push(spawn_night_light_subscription(
        night_light.clone(),
        cancel.clone(),
    ));

    let (config_tx, mut config_rx) = mpsc::channel(1);
    let config_cancel = cancel.clone();
    running_services.push(spawn_service(cancel.clone(), move |_| async move {
        tokio::select! {
            _ = config_cancel.cancelled() => {}
            _ = watch_for_config_changes(config_tx) => {}
        }
    }));

    tracing::info!("glimpse-sunset is running");
    let mut current_config = config;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received shutdown signal");
                break;
            }
            message = config_rx.recv() => match message {
                Some(ConfigEvent::Changed(config)) => {
                    if current_config == config {
                        continue;
                    }
                    tracing::info!("app config changed");
                    reconfigure_services(&location, &night_light, config.clone());
                    current_config = config;
                }
                None => break,
            }
        }
    }

    shutdown_services(&location, &night_light);
    cancel.cancel();
    for service in running_services {
        service.cancel().await;
    }
    tracing::info!("glimpse-sunset stopped");

    Ok(())
}

pub fn start_services(
    location: &location::LocationHandle,
    night_light: &NightLightHandle,
    config: Config,
) {
    send_control("location", location, Control::Start(config.clone()));
    send_control("night-light", night_light, Control::Start(config));
}

pub fn reconfigure_services(
    location: &location::LocationHandle,
    night_light: &NightLightHandle,
    config: Config,
) {
    send_control("location", location, Control::Reconfigure(config.clone()));
    if let Err(error) = night_light.try_send(ServiceCommand::Command(
        night_light::Command::ApplyConfig(config.night_light),
    )) {
        tracing::warn!(%error, "failed to send night light config update");
    }
}

fn shutdown_services(location: &location::LocationHandle, night_light: &NightLightHandle) {
    send_control("location", location, Control::Shutdown);
    send_control("night-light", night_light, Control::Shutdown);
}

fn send_control<State, Command>(
    service_name: &'static str,
    handle: &ServiceHandle<State, Command>,
    control: Control,
) where
    State: Clone,
    Command: Send,
{
    if let Err(error) = handle.try_send(ServiceCommand::Control(control)) {
        tracing::warn!(service = service_name, %error, "failed to send service control");
    }
}

fn spawn_night_light_subscription(
    night_light: NightLightHandle,
    cancel: CancellationToken,
) -> AppTask {
    spawn_service(cancel.clone(), move |_| async move {
        let mut state_rx = night_light.subscribe();
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                changed = state_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    log_night_light_state(&state_rx.borrow().clone());
                }
            }
        }
    })
}

fn log_night_light_state(state: &State) {
    tracing::info!(
        compositor = %state.compositor.name(),
        health = ?state.health,
        schedule = ?state.config.schedule,
        phase = ?state.phase,
        target_temperature_kelvin = state.target_temperature_kelvin,
        effective_temperature_kelvin = state.effective_temperature_kelvin,
        "night light state changed"
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
        Config, NightLightConfig, NightLightSchedule,
        services::{
            framework::{Control, ServiceCommand, ServiceHandle},
            location, night_light,
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
    async fn start_services_sends_start_control_to_location_stack_and_night_light() {
        let (location, mut location_rx) =
            handle::<location::State, location::Command>(location::State::Unknown);
        let (night_light, mut night_light_rx) =
            handle::<night_light::State, night_light::Command>(night_light::State::default());

        start_services(&location, &night_light, Config::default());

        assert!(matches!(
            location_rx.recv().await,
            Some(ServiceCommand::Control(Control::Start(_)))
        ));
        assert!(matches!(
            night_light_rx.recv().await,
            Some(ServiceCommand::Control(Control::Start(_)))
        ));
    }

    #[tokio::test]
    async fn reconfigure_services_updates_location_stack_and_night_light_config() {
        let (location, mut location_rx) =
            handle::<location::State, location::Command>(location::State::Unknown);
        let (night_light, mut night_light_rx) =
            handle::<night_light::State, night_light::Command>(night_light::State::default());
        let config = Config {
            night_light: NightLightConfig {
                schedule: NightLightSchedule::Schedule,
                start_time: Some("18:00".into()),
                end_time: Some("06:00".into()),
                ..NightLightConfig::default()
            },
            ..Config::default()
        };

        reconfigure_services(&location, &night_light, config);

        assert!(matches!(
            location_rx.recv().await,
            Some(ServiceCommand::Control(Control::Reconfigure(_)))
        ));
        assert!(matches!(
            night_light_rx.recv().await,
            Some(ServiceCommand::Command(night_light::Command::ApplyConfig(config)))
                if config.schedule == NightLightSchedule::Schedule
        ));
    }
}
