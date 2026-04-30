use std::time::Duration;

use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{Instant, sleep, timeout},
};
use tokio_util::sync::CancellationToken;

use super::{
    client::{ForecastLocation, WeatherClient, WeatherError, configured_city},
    model::{Command, Config, Location, Snapshot, State},
};
use crate::services::{
    framework::{Control, ServiceCommand, ServiceHandle},
    location,
};

const COMMAND_QUEUE_SIZE: usize = 8;
const LOCATION_WAIT: Duration = Duration::from_secs(5);
const CURRENT_LOCATION_LABEL: &str = "Current location";

pub type WeatherHandle = ServiceHandle<State, Command>;

pub struct WeatherService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    client: WeatherClient,
    location: location::LocationHandle,
}

impl WeatherService {
    pub fn new(location: location::LocationHandle) -> (Self, WeatherHandle) {
        let (state_tx, state_rx) = watch::channel(State::Unknown);
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                state_tx,
                command_rx,
                client: WeatherClient::new(),
                location,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        let mut config = None;
        let mut fetch = None;
        let mut location_rx = self.location.subscribe();
        let refresh_timer = sleep(Duration::MAX);
        tokio::pin!(refresh_timer);
        let mut refresh_scheduled = false;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    abort_fetch(fetch);
                    break;
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(Command::Configure(next))) => {
                        if config.as_ref() != Some(&next) {
                            config = Some(next);
                            abort_fetch(fetch.take());
                            fetch = self.start_fetch(config.clone());
                            refresh_scheduled = false;
                        }
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        abort_fetch(fetch);
                        break;
                    }
                    Some(ServiceCommand::Control(Control::Start(_)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(_))) => {}
                },
                result = async {
                    let Some(fetch) = fetch.as_mut() else {
                        return None;
                    };

                    Some(fetch.await)
                }, if fetch.is_some() => {
                    fetch = None;
                    match result {
                        Some(Ok(Ok(snapshot))) => {
                            self.publish(State::Ready(snapshot));
                        }
                        Some(Ok(Err(error))) => {
                            self.publish(State::Unavailable(error.to_string()));
                        }
                        Some(Err(error)) if error.is_cancelled() => {}
                        Some(Err(error)) => {
                            self.publish(State::Unavailable(format!("weather task failed: {error}")));
                        }
                        None => {}
                    }

                    if let Some(config) = &config {
                        refresh_timer.as_mut().reset(Instant::now() + Duration::from_secs(config.refresh_interval()));
                        refresh_scheduled = true;
                    }
                }
                _ = &mut refresh_timer, if refresh_scheduled => {
                    fetch = self.start_fetch(config.clone());
                    refresh_scheduled = false;
                }
                changed = location_rx.changed(), if fetch.is_none() && uses_location_service(&config) => {
                    if changed.is_ok() && matches!(*location_rx.borrow(), location::State::Ready(_)) {
                        fetch = self.start_fetch(config.clone());
                        refresh_scheduled = false;
                    }
                }
            }
        }
    }

    fn start_fetch(
        &self,
        config: Option<Config>,
    ) -> Option<JoinHandle<Result<Snapshot, WeatherError>>> {
        let Some(config) = config else {
            self.publish(State::Unavailable(
                "weather applet is not configured".into(),
            ));
            return None;
        };

        self.publish(State::Loading);
        let client = self.client.clone();
        let location = self.location.clone();
        Some(tokio::spawn(async move {
            let location = resolve_forecast_location(&config, &location).await?;
            client.fetch_snapshot(location, &config).await
        }))
    }

    fn publish(&self, next: State) {
        self.state_tx.send_if_modified(|state| {
            if *state == next {
                false
            } else {
                *state = next;
                true
            }
        });
    }
}

fn abort_fetch(fetch: Option<JoinHandle<Result<Snapshot, WeatherError>>>) {
    if let Some(fetch) = fetch {
        fetch.abort();
    }
}

async fn resolve_forecast_location(
    config: &Config,
    location: &location::LocationHandle,
) -> Result<ForecastLocation, WeatherError> {
    if let Some(city) = configured_city(config) {
        return Ok(ForecastLocation::City(city));
    }

    let mut subscription = location.subscribe();
    if let Some(location) = location_from_state(&subscription.borrow()) {
        return Ok(ForecastLocation::Coordinates(location));
    }

    request_location_refresh(location).await;
    if let Some(location) = location_from_state(&subscription.borrow()) {
        return Ok(ForecastLocation::Coordinates(location));
    }

    timeout(LOCATION_WAIT, async {
        loop {
            subscription
                .changed()
                .await
                .map_err(|_| WeatherError::Location("location service stopped".into()))?;
            if let Some(location) = location_from_state(&subscription.borrow()) {
                return Ok(ForecastLocation::Coordinates(location));
            }
            if matches!(*subscription.borrow(), location::State::Degraded(_)) {
                return Err(WeatherError::Location(
                    "location service is unavailable".into(),
                ));
            }
        }
    })
    .await
    .map_err(|_| WeatherError::MissingLocation)?
}

async fn request_location_refresh(location: &location::LocationHandle) {
    if let Err(error) = location
        .send(ServiceCommand::Command(location::Command::Refresh))
        .await
    {
        tracing::debug!(%error, "failed to request location refresh for weather");
    }
}

fn location_from_state(state: &location::State) -> Option<Location> {
    match state {
        location::State::Ready(coordinates) => Some(Location {
            latitude: coordinates.latitude,
            longitude: coordinates.longitude,
            city: CURRENT_LOCATION_LABEL.into(),
        }),
        location::State::Unknown | location::State::Refreshing | location::State::Degraded(_) => {
            None
        }
    }
}

fn uses_location_service(config: &Option<Config>) -> bool {
    config
        .as_ref()
        .is_some_and(|config| configured_city(config).is_none())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::location::{Coordinates, State as LocationState};

    #[test]
    fn location_from_state_maps_ready_coordinates() {
        let location = location_from_state(&LocationState::Ready(Coordinates {
            latitude: 52.2298,
            longitude: 21.0118,
        }))
        .unwrap();

        assert_eq!(location.latitude, 52.2298);
        assert_eq!(location.longitude, 21.0118);
        assert_eq!(location.city, "Current location");
    }

    #[test]
    fn uses_location_service_when_city_is_absent() {
        assert!(uses_location_service(&Some(Config::default())));
        assert!(!uses_location_service(&Some(Config {
            city_name: "Warsaw, PL".into(),
            ..Config::default()
        })));
        assert!(!uses_location_service(&None));
    }
}
