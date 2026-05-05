use std::{error::Error, time::Duration};

use async_trait::async_trait;

use crate::{
    DAYLIGHT_TEMPERATURE_KELVIN, NightLightConfig, NightLightHealth, NightLightPhase,
    NightLightSchedule,
    compositors::{CompositorCapabilities, CompositorType},
    services::{
        framework::{Control, ServiceCommand, ServiceHandle},
        location,
    },
};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use self::{
    scheduler::{
        ManualScheduleWindow, SolarScheduleWindow, evaluate_automatic_schedule,
        evaluate_manual_schedule, interpolate_temperature,
    },
    solar::solar_times_for_coordinates,
};

mod scheduler;
mod solar;

const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const APPLY_TRANSITION_DURATION: Duration = Duration::from_millis(1500);
const APPLY_TRANSITION_STEP: Duration = Duration::from_millis(100);
const COMMAND_QUEUE_SIZE: usize = 8;
const LOCATION_UNAVAILABLE_MESSAGE: &str =
    "location coordinates are unavailable for automatic night light";

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

#[async_trait]
pub trait NightLightBackend: Send {
    fn compositor_type(&self) -> CompositorType;
    fn compositor_capabilities(&self) -> CompositorCapabilities;
    async fn apply_temperature(&mut self, temperature_kelvin: u32) -> anyhow::Result<()>;
    async fn reset(&mut self) -> anyhow::Result<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub compositor: CompositorType,
    pub health: NightLightHealth,
    pub config: NightLightConfig,
    pub phase: NightLightPhase,
    pub current_temperature_kelvin: u32,
    pub target_temperature_kelvin: u32,
    pub effective_temperature_kelvin: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            compositor: CompositorType::Unsupported,
            health: NightLightHealth::Starting,
            config: NightLightConfig::default(),
            phase: NightLightPhase::Disabled,
            current_temperature_kelvin: DAYLIGHT_TEMPERATURE_KELVIN,
            target_temperature_kelvin: DAYLIGHT_TEMPERATURE_KELVIN,
            effective_temperature_kelvin: DAYLIGHT_TEMPERATURE_KELVIN,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Refresh,
    ApplyConfig(NightLightConfig),
}

pub type NightLightHandle = ServiceHandle<State, Command>;

pub struct NightLightService {
    backend: Box<dyn NightLightBackend>,
    location: location::LocationHandle,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

impl NightLightService {
    pub fn new(
        backend: Box<dyn NightLightBackend>,
        location: location::LocationHandle,
    ) -> (Self, NightLightHandle) {
        let (state_tx, state_rx) = watch::channel(initial_state(&*backend));
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                backend,
                location,
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        tracing::debug!("night light service started");
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        let mut location_rx = self.location.subscribe();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    self.shutdown().await;
                    break;
                }
                _ = interval.tick() => {
                    self.refresh_from_current_state().await;
                }
                changed = location_rx.changed() => {
                    if changed.is_err() {
                        self.set_error_health("location service subscription closed");
                        break;
                    }
                    self.refresh_from_current_state().await;
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Start(config)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(config))) => {
                        self.apply_config(config.night_light).await;
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        self.shutdown().await;
                        break;
                    }
                    Some(ServiceCommand::Command(Command::Refresh)) => {
                        self.refresh_from_current_state().await;
                    }
                    Some(ServiceCommand::Command(Command::ApplyConfig(config))) => {
                        self.apply_config(config).await;
                    }
                }
            }
        }

        tracing::debug!("night light service quit");
    }

    async fn refresh_from_current_state(&mut self) {
        let config = self.state_tx.borrow().config.clone();
        self.apply_config(config).await;
    }

    async fn apply_config(&mut self, config: NightLightConfig) {
        if let Err(error) = apply_config(
            &mut *self.backend,
            &self.location,
            &self.state_tx,
            config.clone(),
        )
        .await
        {
            let error_message = error.to_string();
            if error_message == LOCATION_UNAVAILABLE_MESSAGE {
                tracing::debug!("night light service: waiting for location coordinates");
            } else {
                tracing::warn!(error = %error, "night light service: apply failed");
            }
            self.state_tx.send_if_modified(|state| {
                if state.config == config {
                    false
                } else {
                    state.config = config;
                    true
                }
            });
            if error_message != LOCATION_UNAVAILABLE_MESSAGE {
                apply_error_health(&self.state_tx, error.as_ref());
            }
        }
    }

    async fn shutdown(&mut self) {
        if let Err(error) = self.backend.reset().await {
            tracing::debug!(%error, "night light service: failed to reset backend during shutdown");
        }
    }

    fn set_error_health(&self, message: impl Into<String>) {
        let mut next_state = self.state_tx.borrow().clone();
        next_state.health = NightLightHealth::Degraded {
            message: message.into(),
        };
        publish_state_if_changed(&self.state_tx, next_state);
    }
}

fn initial_state(backend: &dyn NightLightBackend) -> State {
    State {
        compositor: backend.compositor_type(),
        health: if backend.compositor_capabilities().night_light {
            NightLightHealth::Starting
        } else {
            NightLightHealth::Unsupported
        },
        ..State::default()
    }
}

fn service_error(message: impl Into<String>) -> ServiceError {
    Box::new(std::io::Error::other(message.into()))
}

fn health_for_error(error: &dyn Error) -> NightLightHealth {
    let message = error.to_string();
    if message.contains("gamma-control protocol is unavailable")
        || message.contains("night light backend is unavailable")
        || message.contains("no wayland outputs available for night light")
    {
        NightLightHealth::Unsupported
    } else {
        NightLightHealth::Degraded { message }
    }
}

fn apply_error_health(state_tx: &watch::Sender<State>, error: &dyn Error) {
    let health = health_for_error(error);
    let mut next_state = state_tx.borrow().clone();
    next_state.health = health;
    publish_state_if_changed(state_tx, next_state);
}

async fn apply_config(
    backend: &mut dyn NightLightBackend,
    location: &location::LocationHandle,
    state_tx: &watch::Sender<State>,
    config: NightLightConfig,
) -> ServiceResult<()> {
    let compositor = backend.compositor_type();
    let compositor_capabilities = backend.compositor_capabilities();
    let previous_state = state_tx.borrow().clone();

    if !compositor_capabilities.night_light {
        let mut next_state = previous_state;
        next_state.compositor = compositor;
        next_state.config = config;
        next_state.health = NightLightHealth::Unsupported;
        next_state.phase = NightLightPhase::Disabled;
        next_state.current_temperature_kelvin = DAYLIGHT_TEMPERATURE_KELVIN;
        next_state.target_temperature_kelvin = DAYLIGHT_TEMPERATURE_KELVIN;
        next_state.effective_temperature_kelvin = DAYLIGHT_TEMPERATURE_KELVIN;
        publish_state_if_changed(state_tx, next_state);
        return Ok(());
    }

    let (phase, effective_temperature) = resolve_effective_temperature(&config, location).await?;

    apply_temperature_transition(
        backend,
        previous_state.effective_temperature_kelvin,
        effective_temperature,
    )
    .await?;

    log_state_transition(&previous_state, &config, phase, effective_temperature);

    let target_temperature = config.temperature;
    let next_state = State {
        compositor,
        health: NightLightHealth::Ready,
        config,
        phase,
        current_temperature_kelvin: effective_temperature,
        target_temperature_kelvin: target_temperature,
        effective_temperature_kelvin: effective_temperature,
    };
    publish_state_if_changed(state_tx, next_state);

    Ok(())
}

async fn resolve_effective_temperature(
    config: &NightLightConfig,
    location: &location::LocationHandle,
) -> ServiceResult<(NightLightPhase, u32)> {
    match config.schedule {
        NightLightSchedule::Off => Ok((NightLightPhase::Disabled, DAYLIGHT_TEMPERATURE_KELVIN)),
        NightLightSchedule::Schedule => {
            let start = config
                .start_time
                .as_deref()
                .ok_or_else(|| service_error("scheduled night light start_time is missing"))?;
            let end = config
                .end_time
                .as_deref()
                .ok_or_else(|| service_error("scheduled night light end_time is missing"))?;
            let now = current_local_time();
            let window = ManualScheduleWindow::new(start, end, config.transition_minutes)
                .map_err(service_error)?;
            let evaluation = evaluate_manual_schedule(&window, &now).map_err(service_error)?;
            let effective = interpolate_temperature(
                DAYLIGHT_TEMPERATURE_KELVIN,
                config.temperature,
                evaluation.night_progress,
            );
            tracing::debug!(
                start,
                end,
                now,
                transition_minutes = config.transition_minutes,
                phase = ?evaluation.phase,
                night_progress = evaluation.night_progress,
                effective_temperature_kelvin = effective,
                "night light service: manual schedule evaluated"
            );
            Ok((evaluation.phase, effective))
        }
        NightLightSchedule::Automatic => {
            let coordinates = resolve_coordinates(location).await?;
            let solar_times =
                solar_times_for_coordinates(coordinates.latitude, coordinates.longitude)
                    .map_err(|error| -> ServiceError { error.into() })?;
            let window = SolarScheduleWindow::new(
                &solar_times.sunset,
                &solar_times.sunrise,
                config.transition_minutes,
            )
            .map_err(service_error)?;
            let now = current_local_time();
            let evaluation = evaluate_automatic_schedule(&window, &now).map_err(service_error)?;
            let effective = interpolate_temperature(
                DAYLIGHT_TEMPERATURE_KELVIN,
                config.temperature,
                evaluation.night_progress,
            );
            tracing::debug!(
                latitude = coordinates.latitude,
                longitude = coordinates.longitude,
                sunrise = %solar_times.sunrise,
                sunset = %solar_times.sunset,
                now,
                transition_minutes = config.transition_minutes,
                phase = ?evaluation.phase,
                night_progress = evaluation.night_progress,
                effective_temperature_kelvin = effective,
                "night light service: automatic schedule evaluated"
            );
            Ok((evaluation.phase, effective))
        }
    }
}

fn publish_state_if_changed(state_tx: &watch::Sender<State>, next_state: State) -> bool {
    if *state_tx.borrow() == next_state {
        false
    } else {
        state_tx.send_replace(next_state);
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Coordinates {
    latitude: f64,
    longitude: f64,
}

async fn resolve_coordinates(location: &location::LocationHandle) -> ServiceResult<Coordinates> {
    match location.snapshot() {
        location::State::Ready(coordinates) => {
            tracing::debug!(
                latitude = coordinates.latitude,
                longitude = coordinates.longitude,
                "night light service: using location service coordinates"
            );
            Ok(Coordinates {
                latitude: coordinates.latitude,
                longitude: coordinates.longitude,
            })
        }
        location::State::Unknown | location::State::Refreshing | location::State::Degraded(_) => {
            Err(service_error(LOCATION_UNAVAILABLE_MESSAGE))
        }
    }
}

fn log_state_transition(
    previous_state: &State,
    config: &NightLightConfig,
    phase: NightLightPhase,
    effective_temperature: u32,
) {
    let was_active = previous_state.effective_temperature_kelvin != DAYLIGHT_TEMPERATURE_KELVIN;
    let is_active = effective_temperature != DAYLIGHT_TEMPERATURE_KELVIN;

    if !was_active && is_active {
        tracing::info!(
            schedule = ?config.schedule,
            phase = ?phase,
            target_temperature_kelvin = config.temperature,
            effective_temperature_kelvin = effective_temperature,
            "night light service: activated"
        );
    } else if was_active && !is_active {
        tracing::info!(
            schedule = ?config.schedule,
            previous_phase = ?previous_state.phase,
            phase = ?phase,
            "night light service: deactivated"
        );
    } else if previous_state.phase == phase
        && previous_state.effective_temperature_kelvin == effective_temperature
    {
        tracing::debug!(
            schedule = ?config.schedule,
            phase = ?phase,
            target_temperature_kelvin = config.temperature,
            effective_temperature_kelvin = effective_temperature,
            "night light service: state unchanged after refresh"
        );
    }
}

fn transition_temperatures(from: u32, to: u32) -> Vec<u32> {
    if from == to {
        return Vec::new();
    }

    let step_count = ((APPLY_TRANSITION_DURATION.as_millis() + APPLY_TRANSITION_STEP.as_millis()
        - 1)
        / APPLY_TRANSITION_STEP.as_millis())
    .max(1) as u32;
    let mut temperatures = Vec::with_capacity(step_count as usize);

    for step in 1..=step_count {
        let temperature = interpolate_temperature(from, to, step as f32 / step_count as f32);
        if temperatures.last().copied() != Some(temperature) {
            temperatures.push(temperature);
        }
    }

    temperatures
}

async fn apply_temperature_transition(
    backend: &mut dyn NightLightBackend,
    from: u32,
    to: u32,
) -> ServiceResult<()> {
    let temperatures = transition_temperatures(from, to);

    if temperatures.is_empty() {
        if to == DAYLIGHT_TEMPERATURE_KELVIN {
            apply_temperature_now(backend, to).await?;
        }
        return Ok(());
    }

    let mut previous_temperature = from;
    for (index, temperature) in temperatures.iter().copied().enumerate() {
        apply_temperature_now(backend, temperature).await?;
        tracing::debug!(
            previous_temperature_kelvin = previous_temperature,
            effective_temperature_kelvin = temperature,
            "night light service: temperature changed"
        );
        previous_temperature = temperature;

        if index + 1 < temperatures.len() {
            tokio::time::sleep(APPLY_TRANSITION_STEP).await;
        }
    }

    Ok(())
}

async fn apply_temperature_now(
    backend: &mut dyn NightLightBackend,
    effective_temperature: u32,
) -> ServiceResult<()> {
    if effective_temperature == DAYLIGHT_TEMPERATURE_KELVIN {
        backend
            .reset()
            .await
            .map_err(|error| -> ServiceError { error.into() })?;
    } else {
        backend
            .apply_temperature(effective_temperature)
            .await
            .map_err(|error| -> ServiceError { error.into() })?;
    }

    Ok(())
}

fn current_local_time() -> String {
    chrono::Local::now().format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{
        Coordinates, LOCATION_UNAVAILABLE_MESSAGE, NightLightBackend, State, resolve_coordinates,
        transition_temperatures,
    };
    use crate::{
        Config, NightLightConfig, NightLightHealth, NightLightPhase,
        compositors::{CompositorCapabilities, CompositorType},
        services::{
            framework::{Control, ServiceCommand, ServiceHandle},
            location,
        },
    };
    use async_trait::async_trait;
    use tokio::sync::{mpsc, watch};

    #[derive(Clone, Default)]
    struct BackendLog {
        applied: Vec<u32>,
        resets: usize,
    }

    struct MockBackend {
        log: Arc<Mutex<BackendLog>>,
    }

    #[async_trait]
    impl NightLightBackend for MockBackend {
        fn compositor_type(&self) -> CompositorType {
            CompositorType::Niri
        }

        fn compositor_capabilities(&self) -> CompositorCapabilities {
            CompositorCapabilities {
                night_light: true,
                ..CompositorCapabilities::default()
            }
        }

        async fn apply_temperature(&mut self, temperature_kelvin: u32) -> anyhow::Result<()> {
            self.log.lock().unwrap().applied.push(temperature_kelvin);
            Ok(())
        }

        async fn reset(&mut self) -> anyhow::Result<()> {
            self.log.lock().unwrap().resets += 1;
            Ok(())
        }
    }

    fn location_handle(initial: location::State) -> location::LocationHandle {
        let (_state_tx, state_rx) = watch::channel(initial);
        let (command_tx, _command_rx) = mpsc::channel(4);
        ServiceHandle::new(state_rx, command_tx)
    }

    #[test]
    fn transition_temperatures_use_multiple_steps_for_large_changes() {
        let steps = transition_temperatures(6500, 4200);

        assert!(steps.len() > 1);
        assert_eq!(steps.last().copied(), Some(4200));
        assert!(steps[0] < 6500);
    }

    #[tokio::test]
    async fn location_service_coordinates_are_used_for_automatic_schedule() {
        let location = location_handle(location::State::Ready(location::Coordinates {
            latitude: 40.7128,
            longitude: -74.006,
        }));

        let coordinates = resolve_coordinates(&location).await.expect("coordinates");

        assert_eq!(
            coordinates,
            Coordinates {
                latitude: 40.7128,
                longitude: -74.006
            }
        );
    }

    #[tokio::test]
    async fn unavailable_location_waits_for_location_service_update() {
        let (_state_tx, state_rx) = watch::channel(location::State::Unknown);
        let (command_tx, mut command_rx) = mpsc::channel(4);
        let location = ServiceHandle::new(state_rx, command_tx);

        let error = resolve_coordinates(&location)
            .await
            .expect_err("coordinates should be unavailable");

        assert_eq!(error.to_string(), LOCATION_UNAVAILABLE_MESSAGE);
        assert!(command_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn service_accepts_start_control_config() {
        let log = Arc::new(Mutex::new(BackendLog::default()));
        let backend = Box::new(MockBackend { log: log.clone() });
        let (_location_state_tx, location_state_rx) =
            watch::channel(location::State::Ready(location::Coordinates {
                latitude: 52.2298,
                longitude: 21.0118,
            }));
        let (location_command_tx, _location_command_rx) = mpsc::channel(4);
        let location = ServiceHandle::new(location_state_rx, location_command_tx);
        let (service, handle) = super::NightLightService::new(backend, location);
        let config = Config {
            night_light: NightLightConfig::default(),
            ..Config::default()
        };

        handle
            .send(ServiceCommand::Control(Control::Start(config)))
            .await
            .expect("send start");
        let cancel = tokio_util::sync::CancellationToken::new();
        let service_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            service.run(service_cancel).await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        let state = handle.snapshot();
        assert_eq!(state.health, NightLightHealth::Ready);
        assert_eq!(state.phase, NightLightPhase::Disabled);
        assert!(log.lock().unwrap().resets > 0);
    }

    #[test]
    fn default_state_starts_at_daylight() {
        let state = State::default();

        assert_eq!(state.current_temperature_kelvin, 6500);
        assert_eq!(state.effective_temperature_kelvin, 6500);
    }
}
