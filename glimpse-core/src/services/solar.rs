use std::time::Duration;

use chrono::{Local, NaiveDate};
use sunrise::{Coordinates as SunriseCoordinates, SolarDay, SolarEvent};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::services::{
    framework::{Control, ServiceCommand, ServiceHandle},
    location,
};

const COMMAND_QUEUE_SIZE: usize = 4;
const REFRESH_INTERVAL: Duration = Duration::from_secs(60);
const LOCATION_UNAVAILABLE_MESSAGE: &str = "location coordinates are unavailable for solar service";

#[derive(Debug, Clone, PartialEq)]
pub struct SolarTimes {
    pub sunrise: String,
    pub sunset: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub coordinates: location::Coordinates,
    pub date: NaiveDate,
    pub times: SolarTimes,
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Unknown,
    Ready(Snapshot),
    Degraded { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
}

pub type SolarHandle = ServiceHandle<State, Command>;

pub struct SolarService {
    location: location::LocationHandle,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

impl SolarService {
    pub fn new(location: location::LocationHandle) -> (Self, SolarHandle) {
        let (state_tx, state_rx) = watch::channel(State::Unknown);
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                location,
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        tracing::debug!("solar service started");
        let mut interval = tokio::time::interval(REFRESH_INTERVAL);
        let mut location_rx = self.location.subscribe();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => self.refresh(),
                changed = location_rx.changed() => {
                    if changed.is_err() {
                        self.publish(State::Degraded {
                            message: "location service subscription closed".into(),
                        });
                        break;
                    }
                    self.refresh();
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Start(_)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(_)))
                    | Some(ServiceCommand::Command(Command::Refresh)) => self.refresh(),
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => break,
                }
            }
        }

        tracing::debug!("solar service quit");
    }

    fn refresh(&self) {
        let location::State::Ready(coordinates) = self.location.snapshot() else {
            self.publish(State::Unknown);
            tracing::debug!("solar service: waiting for location coordinates");
            return;
        };

        match solar_times_for_coordinates(coordinates.latitude, coordinates.longitude) {
            Ok(times) => {
                tracing::debug!(
                    latitude = coordinates.latitude,
                    longitude = coordinates.longitude,
                    sunrise = %times.sunrise,
                    sunset = %times.sunset,
                    "solar service: solar times calculated"
                );
                self.publish(State::Ready(Snapshot {
                    coordinates,
                    date: Local::now().date_naive(),
                    times,
                }));
            }
            Err(error) => {
                tracing::warn!(%error, "solar service: failed to calculate solar times");
                self.publish(State::Degraded {
                    message: error.to_string(),
                });
            }
        }
    }

    fn publish(&self, state: State) -> bool {
        self.state_tx.send_if_modified(|current| {
            if *current == state {
                false
            } else {
                *current = state;
                true
            }
        })
    }
}

pub fn solar_times_for_coordinates(latitude: f64, longitude: f64) -> anyhow::Result<SolarTimes> {
    solar_times_for_date(Local::now().date_naive(), latitude, longitude)
}

pub fn solar_times_for_date(
    date: NaiveDate,
    latitude: f64,
    longitude: f64,
) -> anyhow::Result<SolarTimes> {
    let coordinates = validate_coordinates(latitude, longitude)?;
    let coordinates = SunriseCoordinates::new(coordinates.latitude, coordinates.longitude)
        .ok_or_else(|| anyhow::anyhow!("invalid coordinates"))?;
    let solar_day = SolarDay::new(coordinates, date);
    let sunrise = solar_day.event_time(SolarEvent::Sunrise);
    let sunset = solar_day.event_time(SolarEvent::Sunset);

    Ok(SolarTimes {
        sunrise: sunrise.with_timezone(&Local).format("%H:%M").to_string(),
        sunset: sunset.with_timezone(&Local).format("%H:%M").to_string(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordinatesPair {
    pub latitude: f64,
    pub longitude: f64,
}

pub fn validate_coordinates(latitude: f64, longitude: f64) -> anyhow::Result<CoordinatesPair> {
    if !(-90.0..=90.0).contains(&latitude) {
        anyhow::bail!("latitude {latitude} is out of range");
    }
    if !(-180.0..=180.0).contains(&longitude) {
        anyhow::bail!("longitude {longitude} is out of range");
    }

    Ok(CoordinatesPair {
        latitude,
        longitude,
    })
}

pub fn unavailable_message() -> &'static str {
    LOCATION_UNAVAILABLE_MESSAGE
}

#[cfg(test)]
mod tests {
    use super::{SolarService, State, solar_times_for_coordinates, validate_coordinates};
    use crate::services::{
        framework::{Control, ServiceCommand, ServiceHandle},
        location,
    };
    use tokio::sync::{mpsc, watch};
    use tokio_util::sync::CancellationToken;

    fn location_handle(
        initial: location::State,
    ) -> (watch::Sender<location::State>, location::LocationHandle) {
        let (state_tx, state_rx) = watch::channel(initial);
        let (command_tx, _command_rx) = mpsc::channel(4);
        (state_tx, ServiceHandle::new(state_rx, command_tx))
    }

    #[test]
    fn solar_times_format_clock_strings() {
        let times = solar_times_for_coordinates(52.2298, 21.0118).expect("solar times");

        assert_eq!(times.sunrise.len(), 5);
        assert_eq!(times.sunset.len(), 5);
        assert!(times.sunrise.contains(':'));
        assert!(times.sunset.contains(':'));
    }

    #[test]
    fn coordinates_are_validated() {
        assert!(validate_coordinates(52.2298, 21.0118).is_ok());
        assert!(validate_coordinates(91.0, 21.0118).is_err());
        assert!(validate_coordinates(52.2298, 181.0).is_err());
    }

    #[tokio::test]
    async fn solar_service_uses_location_service_coordinates() {
        let (_location_tx, location) =
            location_handle(location::State::Ready(location::Coordinates {
                latitude: 52.2298,
                longitude: 21.0118,
            }));
        let (service, handle) = SolarService::new(location);
        let cancel = CancellationToken::new();
        let service_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(service_cancel).await });

        handle
            .send(ServiceCommand::Control(Control::Start(
                crate::Config::default(),
            )))
            .await
            .expect("start solar service");
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        assert!(matches!(handle.snapshot(), State::Ready(_)));
    }

    #[tokio::test]
    async fn solar_service_waits_for_location_coordinates() {
        let (_location_tx, location) = location_handle(location::State::Unknown);
        let (service, handle) = SolarService::new(location);
        let cancel = CancellationToken::new();
        let service_cancel = cancel.clone();
        let task = tokio::spawn(async move { service.run(service_cancel).await });

        handle
            .send(ServiceCommand::Control(Control::Start(
                crate::Config::default(),
            )))
            .await
            .expect("start solar service");
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        cancel.cancel();
        let _ = task.await;

        assert_eq!(handle.snapshot(), State::Unknown);
    }
}
