use std::{error::Error, sync::Arc, time::Duration};

use tokio::sync::{mpsc, watch};

use crate::compositor::detect;
use crate::night_light::{
    backend::{NightLightBackend, create_backend},
    protocol::{
        DAYLIGHT_TEMPERATURE_KELVIN, NightLightCommand, NightLightConfig, NightLightHealth,
        NightLightSchedule, NightLightState,
    },
    scheduler::{
        ManualScheduleWindow, SolarScheduleWindow, evaluate_automatic_schedule,
        evaluate_manual_schedule, interpolate_temperature,
    },
};
use crate::solar::provider::{SolarTimesProvider, SolarTimesSource};

const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

type ServiceError = Box<dyn Error + Send + Sync>;
type ServiceResult<T> = Result<T, ServiceError>;

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

fn apply_error_health(state_tx: &watch::Sender<NightLightState>, error: &dyn Error) {
    let health = health_for_error(error);
    state_tx.send_modify(|state| state.health = health.clone());
}

fn night_light_is_active(effective_temperature: u32) -> bool {
    effective_temperature != DAYLIGHT_TEMPERATURE_KELVIN
}

#[derive(Clone)]
pub struct NightLightServiceHandle {
    commands: mpsc::Sender<NightLightCommand>,
    state: watch::Receiver<NightLightState>,
}

impl NightLightServiceHandle {
    pub fn new(config: NightLightConfig) -> Self {
        let compositor = detect();
        let backend = create_backend(compositor);
        let solar_times: Arc<dyn SolarTimesSource> = Arc::new(SolarTimesProvider::default());
        Self::from_parts_with_config(backend, solar_times, config)
    }

    pub(crate) fn from_parts_with_config(
        backend: Box<dyn NightLightBackend>,
        solar_times: Arc<dyn SolarTimesSource>,
        config: NightLightConfig,
    ) -> Self {
        let (state_tx, state) = watch::channel(initial_state(&*backend, config.clone()));
        let (commands, cmd_rx) = mpsc::channel(32);

        tokio::spawn(async move {
            run_night_light_service(backend, solar_times, state_tx, cmd_rx, config).await;
        });

        Self { commands, state }
    }

    pub fn subscribe(&self) -> watch::Receiver<NightLightState> {
        self.state.clone()
    }

    pub async fn send(
        &self,
        command: NightLightCommand,
    ) -> Result<(), mpsc::error::SendError<NightLightCommand>> {
        self.commands.send(command).await
    }
}

fn initial_state(backend: &dyn NightLightBackend, config: NightLightConfig) -> NightLightState {
    let compositor = backend.compositor();
    let compositor_capabilities = backend.compositor_capabilities();
    NightLightState {
        compositor,
        config,
        health: if compositor_capabilities.night_light {
            NightLightHealth::Starting
        } else {
            NightLightHealth::Unsupported
        },
        ..NightLightState::default()
    }
}

async fn run_night_light_service(
    mut backend: Box<dyn NightLightBackend>,
    solar_times: Arc<dyn SolarTimesSource>,
    state_tx: watch::Sender<NightLightState>,
    mut cmd_rx: mpsc::Receiver<NightLightCommand>,
    initial_config: NightLightConfig,
) {
    if !backend.compositor_capabilities().night_light {
        if let Err(error) = apply_config(
            &mut *backend,
            solar_times.as_ref(),
            &state_tx,
            initial_config,
        )
        .await
        {
            tracing::debug!(error = %error, "night light service: unsupported backend bootstrap");
        }
        return;
    }

    if let Err(error) = apply_config(
        &mut *backend,
        solar_times.as_ref(),
        &state_tx,
        initial_config,
    )
    .await
    {
        tracing::warn!(error = %error, "night light service: initial refresh failed");
        apply_error_health(&state_tx, error.as_ref());
    }

    let mut interval = tokio::time::interval(REFRESH_INTERVAL);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let config = state_tx.borrow().config.clone();
                if let Err(error) = apply_config(&mut *backend, solar_times.as_ref(), &state_tx, config).await {
                    tracing::warn!(error = %error, "night light service: periodic refresh failed");
                    apply_error_health(&state_tx, error.as_ref());
                }
            }
            maybe_command = cmd_rx.recv() => {
                match maybe_command {
                    Some(NightLightCommand::Refresh) => {
                        let config = state_tx.borrow().config.clone();
                        if let Err(error) = apply_config(&mut *backend, solar_times.as_ref(), &state_tx, config).await {
                            tracing::warn!(error = %error, "night light service: refresh failed");
                            apply_error_health(&state_tx, error.as_ref());
                        }
                    }
                    Some(NightLightCommand::ApplyConfig(config)) => {
                        if let Err(error) = apply_config(&mut *backend, solar_times.as_ref(), &state_tx, config).await {
                            tracing::warn!(error = %error, "night light service: apply failed");
                            apply_error_health(&state_tx, error.as_ref());
                        }
                    }
                    None => break,
                }
            }
        }
    }
}

async fn apply_config(
    backend: &mut dyn NightLightBackend,
    solar_times: &dyn SolarTimesSource,
    state_tx: &watch::Sender<NightLightState>,
    config: NightLightConfig,
) -> ServiceResult<()> {
    let compositor = backend.compositor();
    let compositor_capabilities = backend.compositor_capabilities();
    state_tx.send_modify(|state| {
        state.compositor = compositor;
        state.config = config.clone();
    });

    if !compositor_capabilities.night_light {
        state_tx.send_modify(|state| {
            state.health = NightLightHealth::Unsupported;
            state.phase = crate::night_light::protocol::NightLightPhase::Disabled;
            state.current_temperature_kelvin = DAYLIGHT_TEMPERATURE_KELVIN;
            state.target_temperature_kelvin = DAYLIGHT_TEMPERATURE_KELVIN;
            state.effective_temperature_kelvin = DAYLIGHT_TEMPERATURE_KELVIN;
        });
        return Ok(());
    }

    let (phase, effective_temperature) = match config.schedule {
        NightLightSchedule::Off => {
            backend
                .reset()
                .await
                .map_err(|error| -> ServiceError { error.into() })?;
            (
                crate::night_light::protocol::NightLightPhase::Disabled,
                DAYLIGHT_TEMPERATURE_KELVIN,
            )
        }
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
            apply_temperature(backend, effective).await?;
            (evaluation.phase, effective)
        }
        NightLightSchedule::Automatic => {
            let solar_times = solar_times
                .resolve_solar_times(config.latitude, config.longitude)
                .await
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
            apply_temperature(backend, effective).await?;
            (evaluation.phase, effective)
        }
    };

    let previous_state = state_tx.borrow().clone();
    let was_active = night_light_is_active(previous_state.effective_temperature_kelvin);
    let is_active = night_light_is_active(effective_temperature);

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
    }

    if previous_state.effective_temperature_kelvin != effective_temperature {
        tracing::info!(
            schedule = ?config.schedule,
            previous_phase = ?previous_state.phase,
            phase = ?phase,
            previous_temperature_kelvin = previous_state.effective_temperature_kelvin,
            effective_temperature_kelvin = effective_temperature,
            target_temperature_kelvin = config.temperature,
            "night light service: temperature changed"
        );
    } else {
        tracing::debug!(
            schedule = ?config.schedule,
            phase = ?phase,
            target_temperature_kelvin = config.temperature,
            effective_temperature_kelvin = effective_temperature,
            "night light service: state unchanged after refresh"
        );
    }

    state_tx.send_modify(|state| {
        state.compositor = compositor;
        state.config = config.clone();
        state.health = NightLightHealth::Ready;
        state.phase = phase;
        state.current_temperature_kelvin = effective_temperature;
        state.target_temperature_kelvin = config.temperature;
        state.effective_temperature_kelvin = effective_temperature;
    });

    Ok(())
}

async fn apply_temperature(
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
    use std::{
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use super::{NightLightServiceHandle, apply_config, initial_state};
    use crate::{
        compositor::{CompositorCapabilities, CompositorKind},
        night_light::{
            backend::{NightLightBackend, UnsupportedNightLightBackend},
            protocol::{
                DAYLIGHT_TEMPERATURE_KELVIN, NightLightCommand, NightLightConfig, NightLightHealth,
                NightLightPhase, NightLightSchedule,
            },
        },
        solar::provider::{SolarTimes, SolarTimesSource},
    };
    use async_trait::async_trait;
    use tokio::sync::watch;

    #[derive(Clone, Default)]
    struct BackendLog {
        applied: Vec<u32>,
        resets: usize,
    }

    struct MockBackend {
        compositor: CompositorKind,
        compositor_capabilities: CompositorCapabilities,
        log: Arc<Mutex<BackendLog>>,
    }

    impl MockBackend {
        fn supported(log: Arc<Mutex<BackendLog>>) -> Self {
            Self {
                compositor: CompositorKind::Niri,
                compositor_capabilities: CompositorKind::Niri.capabilities(),
                log,
            }
        }
    }

    #[async_trait]
    impl NightLightBackend for MockBackend {
        fn compositor(&self) -> CompositorKind {
            self.compositor
        }

        fn compositor_capabilities(&self) -> CompositorCapabilities {
            self.compositor_capabilities
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

    struct MockSolarTimesProvider {
        solar_times: SolarTimes,
    }

    #[async_trait]
    impl SolarTimesSource for MockSolarTimesProvider {
        async fn resolve_solar_times(
            &self,
            _latitude: Option<f64>,
            _longitude: Option<f64>,
        ) -> anyhow::Result<SolarTimes> {
            Ok(self.solar_times.clone())
        }
    }

    struct FailingSolarTimesProvider {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl SolarTimesSource for FailingSolarTimesProvider {
        async fn resolve_solar_times(
            &self,
            _latitude: Option<f64>,
            _longitude: Option<f64>,
        ) -> anyhow::Result<SolarTimes> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("failed to resolve solar times")
        }
    }

    #[tokio::test]
    async fn off_schedule_resets_backend_and_disables_phase() {
        let log = Arc::new(Mutex::new(BackendLog::default()));
        let mut backend = MockBackend::supported(log.clone());
        let solar_times = MockSolarTimesProvider {
            solar_times: SolarTimes {
                sunrise: "06:00".into(),
                sunset: "18:00".into(),
            },
        };
        let (state_tx, state_rx) =
            watch::channel(initial_state(&backend, NightLightConfig::default()));

        apply_config(
            &mut backend,
            &solar_times,
            &state_tx,
            NightLightConfig::default(),
        )
        .await
        .unwrap();

        let state = state_rx.borrow().clone();
        assert_eq!(state.health, NightLightHealth::Ready);
        assert_eq!(state.phase, NightLightPhase::Disabled);
        assert_eq!(
            state.current_temperature_kelvin,
            DAYLIGHT_TEMPERATURE_KELVIN
        );
        assert_eq!(log.lock().unwrap().resets, 1);
    }

    #[tokio::test]
    async fn apply_config_updates_state_and_allows_refresh_command() {
        let log = Arc::new(Mutex::new(BackendLog::default()));
        let backend = Box::new(MockBackend::supported(log.clone()));
        let solar_times = Arc::new(MockSolarTimesProvider {
            solar_times: SolarTimes {
                sunrise: "06:00".into(),
                sunset: "18:00".into(),
            },
        });
        let handle = NightLightServiceHandle::from_parts_with_config(
            backend,
            solar_times,
            NightLightConfig::default(),
        );

        handle
            .send(NightLightCommand::ApplyConfig(NightLightConfig {
                temperature: 4500,
                schedule: NightLightSchedule::Automatic,
                latitude: None,
                longitude: None,
                start_time: None,
                end_time: None,
                transition_minutes: 15,
            }))
            .await
            .unwrap();
        handle.send(NightLightCommand::Refresh).await.unwrap();

        tokio::time::sleep(Duration::from_millis(25)).await;

        let state = handle.subscribe().borrow().clone();
        assert_eq!(state.health, NightLightHealth::Ready);
        assert!(
            matches!(
                state.phase,
                NightLightPhase::Day
                    | NightLightPhase::Night
                    | NightLightPhase::TransitionToNight
                    | NightLightPhase::TransitionToDay
            ),
            "unexpected phase: {:?}",
            state.phase
        );
        assert!(!log.lock().unwrap().applied.is_empty() || log.lock().unwrap().resets > 0);
    }

    #[tokio::test]
    async fn unsupported_backend_maps_to_unsupported_health() {
        let backend = Box::new(UnsupportedNightLightBackend::new(CompositorKind::Unknown));
        let solar_times = Arc::new(MockSolarTimesProvider {
            solar_times: SolarTimes {
                sunrise: "06:00".into(),
                sunset: "18:00".into(),
            },
        });
        let handle = NightLightServiceHandle::from_parts_with_config(
            backend,
            solar_times,
            NightLightConfig::default(),
        );

        tokio::time::sleep(Duration::from_millis(10)).await;

        let state = handle.subscribe().borrow().clone();
        assert_eq!(state.health, NightLightHealth::Unsupported);
        assert_eq!(state.phase, NightLightPhase::Disabled);
    }

    #[tokio::test]
    async fn failed_apply_persists_requested_config_for_followup_refresh() {
        let mut backend = MockBackend::supported(Arc::new(Mutex::new(BackendLog::default())));
        let calls = Arc::new(AtomicUsize::new(0));
        let solar_times = FailingSolarTimesProvider {
            calls: calls.clone(),
        };
        let (state_tx, state_rx) =
            watch::channel(initial_state(&backend, NightLightConfig::default()));
        let failed_config = NightLightConfig {
            temperature: 4500,
            schedule: NightLightSchedule::Automatic,
            latitude: Some(52.2298),
            longitude: Some(21.0118),
            start_time: None,
            end_time: None,
            transition_minutes: 15,
        };

        let error = apply_config(&mut backend, &solar_times, &state_tx, failed_config.clone())
            .await
            .expect_err("apply should fail");

        let state = state_rx.borrow().clone();
        assert_eq!(state.config, failed_config);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(error.to_string(), "failed to resolve solar times");
    }
}
