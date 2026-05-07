use std::time::Duration;

use glimpse_core::{
    Config, ConfigEvent, LocationConfig, NightLightConfig,
    services::{
        framework::Control,
        location,
        night_light::{self, NightLightHandle, NightLightService, State},
        solar,
    },
    watch_for_config_changes,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    backend::create_backend,
    logind::{self, SleepEvent},
};

struct AppTask {
    cancel: CancellationToken,
    task: tokio::task::JoinHandle<()>,
}

#[derive(Debug, Clone, PartialEq)]
struct SunsetAppConfig {
    location: LocationConfig,
    night_light: NightLightConfig,
}

impl SunsetAppConfig {
    fn from_shared(config: &Config) -> Self {
        Self {
            location: config.location.clone(),
            night_light: config.night_light.clone(),
        }
    }
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

    let (solar_service, solar) = solar::SolarService::new(location.clone());
    running_services.push(spawn_service(cancel.clone(), |cancel| {
        solar_service.run(cancel)
    }));

    let backend = create_backend(glimpse_core::compositors::detect_compositor());
    let (night_light_service, night_light) = NightLightService::new(backend, solar.clone());
    running_services.push(spawn_service(cancel.clone(), |cancel| {
        night_light_service.run(cancel)
    }));

    start_services(&location, &solar, &night_light, config.clone());
    running_services.push(spawn_night_light_subscription(
        night_light.clone(),
        cancel.clone(),
    ));
    let (sleep_tx, mut sleep_rx) = mpsc::channel(4);
    running_services.push(spawn_logind_sleep_watcher(sleep_tx, cancel.clone()));

    let (config_tx, mut config_rx) = mpsc::channel(1);
    let config_cancel = cancel.clone();
    running_services.push(spawn_service(cancel.clone(), move |_| async move {
        tokio::select! {
            _ = config_cancel.cancelled() => {}
            _ = watch_for_config_changes(config_tx) => {}
        }
    }));

    tracing::info!("glimpse-sunset is running");
    let mut current_config = SunsetAppConfig::from_shared(&config);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received shutdown signal");
                break;
            }
            message = config_rx.recv() => match message {
                Some(ConfigEvent::Changed(config)) => {
                    let next_config = SunsetAppConfig::from_shared(&config);
                    if current_config == next_config {
                        continue;
                    }
                    tracing::info!("sunset config changed");
                    reconfigure_services(&location, &solar, &night_light, config.clone());
                    current_config = next_config;
                }
                None => break,
            },
            sleep_event = sleep_rx.recv() => match sleep_event {
                Some(SleepEvent::Suspending) => {
                    tracing::info!("system is suspending");
                }
                Some(SleepEvent::Resumed) => {
                    tracing::info!("system resumed; refreshing sunset state");
                    refresh_after_resume(&location, &solar, &night_light);
                }
                None => break,
            }
        }
    }

    shutdown_services(&location, &solar, &night_light);
    cancel.cancel();
    for service in running_services {
        service.cancel().await;
    }
    tracing::info!("glimpse-sunset stopped");

    Ok(())
}

pub fn start_services(
    location: &location::LocationHandle,
    solar: &solar::SolarHandle,
    night_light: &NightLightHandle,
    config: Config,
) {
    location.try_send_control(
        "location",
        Control::Start(config.clone()),
        "failed to send service control",
    );
    solar.try_send_control(
        "solar",
        Control::Start(config.clone()),
        "failed to send service control",
    );
    night_light.try_send_control(
        "night-light",
        Control::Start(config),
        "failed to send service control",
    );
}

pub fn reconfigure_services(
    location: &location::LocationHandle,
    solar: &solar::SolarHandle,
    night_light: &NightLightHandle,
    config: Config,
) {
    location.try_send_control(
        "location",
        Control::Reconfigure(config.clone()),
        "failed to send service control",
    );
    solar.try_send_control(
        "solar",
        Control::Reconfigure(config.clone()),
        "failed to send service control",
    );
    night_light.try_send_command(
        "night-light",
        night_light::Command::ApplyConfig(config.night_light),
        "failed to send night light config update",
    );
}

pub fn refresh_after_resume(
    location: &location::LocationHandle,
    solar: &solar::SolarHandle,
    night_light: &NightLightHandle,
) {
    location.try_send_command(
        "location",
        location::Command::Refresh,
        "failed to send service command",
    );
    solar.try_send_command(
        "solar",
        solar::Command::Refresh,
        "failed to send service command",
    );
    night_light.try_send_command(
        "night-light",
        night_light::Command::Refresh,
        "failed to send service command",
    );
}

fn shutdown_services(
    location: &location::LocationHandle,
    solar: &solar::SolarHandle,
    night_light: &NightLightHandle,
) {
    location.try_send_control(
        "location",
        Control::Shutdown,
        "failed to send service control",
    );
    solar.try_send_control("solar", Control::Shutdown, "failed to send service control");
    night_light.try_send_control(
        "night-light",
        Control::Shutdown,
        "failed to send service control",
    );
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

fn spawn_logind_sleep_watcher(
    sender: mpsc::Sender<SleepEvent>,
    cancel: CancellationToken,
) -> AppTask {
    spawn_service(cancel.clone(), move |task_cancel| async move {
        let mut retry_delay = Duration::from_secs(1);
        loop {
            tokio::select! {
                _ = task_cancel.cancelled() => break,
                result = logind::watch_sleep_events(sender.clone()) => {
                    match result {
                        Ok(()) => {
                            retry_delay = Duration::from_secs(1);
                        }
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                retry_delay_ms = retry_delay.as_millis(),
                                "logind sleep watcher stopped; retrying"
                            );
                            tokio::select! {
                                _ = task_cancel.cancelled() => break,
                                _ = tokio::time::sleep(retry_delay) => {}
                            }
                            retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
                        }
                    }
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
    use super::{reconfigure_services, refresh_after_resume, start_services};
    use glimpse_core::{
        Config, NightLightConfig, NightLightSchedule,
        services::{
            framework::{Control, ServiceCommand, ServiceHandle},
            location, night_light, solar,
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
        let (solar, mut solar_rx) = handle::<solar::State, solar::Command>(solar::State::Unknown);
        let (night_light, mut night_light_rx) =
            handle::<night_light::State, night_light::Command>(night_light::State::default());

        start_services(&location, &solar, &night_light, Config::default());

        assert!(matches!(
            location_rx.recv().await,
            Some(ServiceCommand::Control(Control::Start(_)))
        ));
        assert!(matches!(
            solar_rx.recv().await,
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
        let (solar, mut solar_rx) = handle::<solar::State, solar::Command>(solar::State::Unknown);
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

        reconfigure_services(&location, &solar, &night_light, config);

        assert!(matches!(
            location_rx.recv().await,
            Some(ServiceCommand::Control(Control::Reconfigure(_)))
        ));
        assert!(matches!(
            solar_rx.recv().await,
            Some(ServiceCommand::Control(Control::Reconfigure(_)))
        ));
        assert!(matches!(
            night_light_rx.recv().await,
            Some(ServiceCommand::Command(night_light::Command::ApplyConfig(config)))
                if config.schedule == NightLightSchedule::Schedule
        ));
    }

    #[tokio::test]
    async fn refresh_after_resume_refreshes_location_stack_and_night_light() {
        let (location, mut location_rx) =
            handle::<location::State, location::Command>(location::State::Unknown);
        let (solar, mut solar_rx) = handle::<solar::State, solar::Command>(solar::State::Unknown);
        let (night_light, mut night_light_rx) =
            handle::<night_light::State, night_light::Command>(night_light::State::default());

        refresh_after_resume(&location, &solar, &night_light);

        assert!(matches!(
            location_rx.recv().await,
            Some(ServiceCommand::Command(location::Command::Refresh))
        ));
        assert!(matches!(
            solar_rx.recv().await,
            Some(ServiceCommand::Command(solar::Command::Refresh))
        ));
        assert!(matches!(
            night_light_rx.recv().await,
            Some(ServiceCommand::Command(night_light::Command::Refresh))
        ));
    }
}
