use std::time::Duration;

use chrono::{Local, NaiveTime};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    Config, ThemeMode,
    services::{
        framework::{Control, ServiceCommand, ServiceHandle},
        solar,
    },
};

const COMMAND_QUEUE_SIZE: usize = 8;
const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveThemeMode {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeReason {
    Config,
    SolarDay,
    SolarNight,
    SolarUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub configured_mode: ThemeMode,
    pub effective_mode: EffectiveThemeMode,
    pub reason: ThemeReason,
}

impl Default for State {
    fn default() -> Self {
        Self {
            configured_mode: ThemeMode::Auto,
            effective_mode: EffectiveThemeMode::Light,
            reason: ThemeReason::SolarUnavailable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    SetMode(ThemeMode),
    Refresh,
}

pub type ThemeHandle = ServiceHandle<State, Command>;

pub struct ThemeService {
    solar: solar::SolarHandle,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

impl ThemeService {
    pub fn new(solar: solar::SolarHandle) -> (Self, ThemeHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                solar,
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        tracing::debug!("theme service started");
        let mut solar_rx = self.solar.subscribe();
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        interval.tick().await;
        self.refresh();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    self.refresh();
                }
                changed = solar_rx.changed() => {
                    if changed.is_err() {
                        self.publish(resolve_effective_mode(
                            self.state_tx.borrow().configured_mode,
                            &solar::State::Unknown,
                            Local::now().time(),
                            Some(self.state_tx.borrow().effective_mode),
                        ));
                        break;
                    }
                    self.refresh();
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Start(config)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(config))) => {
                        self.apply_config(&config);
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => break,
                    Some(ServiceCommand::Command(Command::SetMode(mode))) => {
                        self.set_mode(mode);
                    }
                    Some(ServiceCommand::Command(Command::Refresh)) => {
                        self.refresh();
                    }
                }
            }
        }

        tracing::debug!("theme service quit");
    }

    fn apply_config(&self, config: &Config) {
        self.set_mode(config.theme_mode);
    }

    fn set_mode(&self, mode: ThemeMode) {
        let previous = self.state_tx.borrow().effective_mode;
        let next = resolve_effective_mode(
            mode,
            &self.solar.snapshot(),
            Local::now().time(),
            Some(previous),
        );
        self.publish(next);
    }

    fn refresh(&self) {
        let current = self.state_tx.borrow();
        let next = resolve_effective_mode(
            current.configured_mode,
            &self.solar.snapshot(),
            Local::now().time(),
            Some(current.effective_mode),
        );
        drop(current);
        self.publish(next);
    }

    fn publish(&self, next: State) {
        let previous = *self.state_tx.borrow();
        let changed = self.state_tx.send_if_modified(|current| {
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        });

        if changed {
            tracing::info!(
                configured_mode = ?next.configured_mode,
                effective_mode = ?next.effective_mode,
                reason = ?next.reason,
                previous_configured_mode = ?previous.configured_mode,
                previous_effective_mode = ?previous.effective_mode,
                "theme state changed"
            );
        }
    }
}

pub fn resolve_effective_mode(
    configured_mode: ThemeMode,
    solar: &solar::State,
    now: NaiveTime,
    previous_effective_mode: Option<EffectiveThemeMode>,
) -> State {
    match configured_mode {
        ThemeMode::Light => State {
            configured_mode,
            effective_mode: EffectiveThemeMode::Light,
            reason: ThemeReason::Config,
        },
        ThemeMode::Dark => State {
            configured_mode,
            effective_mode: EffectiveThemeMode::Dark,
            reason: ThemeReason::Config,
        },
        ThemeMode::Auto => resolve_auto_mode(solar, now, previous_effective_mode),
    }
}

fn resolve_auto_mode(
    solar: &solar::State,
    now: NaiveTime,
    previous_effective_mode: Option<EffectiveThemeMode>,
) -> State {
    let Some(daylight) = daylight_now(solar, now) else {
        return State {
            configured_mode: ThemeMode::Auto,
            effective_mode: previous_effective_mode.unwrap_or(EffectiveThemeMode::Light),
            reason: ThemeReason::SolarUnavailable,
        };
    };

    if daylight {
        State {
            configured_mode: ThemeMode::Auto,
            effective_mode: EffectiveThemeMode::Light,
            reason: ThemeReason::SolarDay,
        }
    } else {
        State {
            configured_mode: ThemeMode::Auto,
            effective_mode: EffectiveThemeMode::Dark,
            reason: ThemeReason::SolarNight,
        }
    }
}

fn daylight_now(solar: &solar::State, now: NaiveTime) -> Option<bool> {
    let solar::State::Ready(snapshot) = solar else {
        return None;
    };
    let sunrise = NaiveTime::parse_from_str(&snapshot.times.sunrise, "%H:%M").ok()?;
    let sunset = NaiveTime::parse_from_str(&snapshot.times.sunset, "%H:%M").ok()?;

    Some(now >= sunrise && now < sunset)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveTime;

    use crate::{
        ThemeMode,
        services::{
            framework::{Control, ServiceCommand, ServiceHandle},
            location, solar,
        },
    };
    use tokio::sync::{mpsc, watch};
    use tokio_util::sync::CancellationToken;

    use super::{
        Command, EffectiveThemeMode, State, ThemeReason, ThemeService, resolve_effective_mode,
    };

    fn solar_ready(sunrise: &str, sunset: &str) -> solar::State {
        solar::State::Ready(solar::Snapshot {
            coordinates: location::Coordinates {
                latitude: 52.23,
                longitude: 21.01,
            },
            date: chrono::Local::now().date_naive(),
            times: solar::SolarTimes {
                sunrise: sunrise.into(),
                sunset: sunset.into(),
            },
        })
    }

    fn solar_handle(initial: solar::State) -> (watch::Sender<solar::State>, solar::SolarHandle) {
        let (state_tx, state_rx) = watch::channel(initial);
        let (command_tx, _command_rx) = mpsc::channel(4);
        (state_tx, ServiceHandle::new(state_rx, command_tx))
    }

    #[test]
    fn explicit_light_mode_ignores_solar() {
        let state = resolve_effective_mode(
            ThemeMode::Light,
            &solar_ready("06:00", "18:00"),
            NaiveTime::from_hms_opt(23, 0, 0).unwrap(),
            Some(EffectiveThemeMode::Dark),
        );

        assert_eq!(
            state,
            State {
                configured_mode: ThemeMode::Light,
                effective_mode: EffectiveThemeMode::Light,
                reason: ThemeReason::Config,
            }
        );
    }

    #[test]
    fn auto_mode_uses_light_between_sunrise_and_sunset() {
        let state = resolve_effective_mode(
            ThemeMode::Auto,
            &solar_ready("06:00", "18:00"),
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            None,
        );

        assert_eq!(state.effective_mode, EffectiveThemeMode::Light);
        assert_eq!(state.reason, ThemeReason::SolarDay);
    }

    #[test]
    fn auto_mode_uses_dark_outside_daylight() {
        let state = resolve_effective_mode(
            ThemeMode::Auto,
            &solar_ready("06:00", "18:00"),
            NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            None,
        );

        assert_eq!(state.effective_mode, EffectiveThemeMode::Dark);
        assert_eq!(state.reason, ThemeReason::SolarNight);
    }

    #[test]
    fn auto_mode_keeps_previous_effective_mode_when_solar_is_unavailable() {
        let state = resolve_effective_mode(
            ThemeMode::Auto,
            &solar::State::Unknown,
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            Some(EffectiveThemeMode::Dark),
        );

        assert_eq!(state.effective_mode, EffectiveThemeMode::Dark);
        assert_eq!(state.reason, ThemeReason::SolarUnavailable);
    }

    #[tokio::test]
    async fn set_mode_command_updates_service_state() {
        let (_solar_tx, solar) = solar_handle(solar_ready("06:00", "18:00"));
        let (service, handle) = ThemeService::new(solar);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(service.run(cancel.clone()));
        let mut state_rx = handle.subscribe();

        handle
            .send(ServiceCommand::Control(Control::Start(crate::Config {
                theme_mode: ThemeMode::Auto,
                ..crate::Config::default()
            })))
            .await
            .unwrap();
        handle
            .send(ServiceCommand::Command(Command::SetMode(ThemeMode::Dark)))
            .await
            .unwrap();

        wait_for_state(&mut state_rx, |state| {
            state.configured_mode == ThemeMode::Dark
                && state.effective_mode == EffectiveThemeMode::Dark
        })
        .await;
        assert_eq!(state_rx.borrow().configured_mode, ThemeMode::Dark);
        assert_eq!(state_rx.borrow().effective_mode, EffectiveThemeMode::Dark);

        cancel.cancel();
        task.await.unwrap();
    }

    async fn wait_for_state(
        state_rx: &mut watch::Receiver<State>,
        predicate: impl Fn(&State) -> bool,
    ) {
        loop {
            if predicate(&state_rx.borrow()) {
                return;
            }
            state_rx.changed().await.unwrap();
        }
    }
}
