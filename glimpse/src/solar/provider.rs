use crate::location::provider::{LocationProvider, validate_coordinates};
use async_trait::async_trait;
use chrono::{Local, NaiveDate};
use sunrise::{Coordinates, SolarDay, SolarEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolarTimes {
    pub sunrise: String,
    pub sunset: String,
}

#[async_trait]
pub trait SolarTimesSource: Send + Sync {
    async fn resolve_solar_times(
        &self,
        latitude: Option<f64>,
        longitude: Option<f64>,
    ) -> anyhow::Result<SolarTimes>;
}

#[derive(Clone, Default)]
pub struct SolarTimesProvider {
    locations: LocationProvider,
}

impl SolarTimesProvider {
    #[cfg(test)]
    pub fn with_location_provider(locations: LocationProvider) -> Self {
        Self { locations }
    }
}

#[async_trait]
impl SolarTimesSource for SolarTimesProvider {
    async fn resolve_solar_times(
        &self,
        latitude: Option<f64>,
        longitude: Option<f64>,
    ) -> anyhow::Result<SolarTimes> {
        let coordinates = if let (Some(latitude), Some(longitude)) = (latitude, longitude) {
            let coordinates = validate_coordinates(latitude, longitude)?;
            tracing::info!(
                latitude = coordinates.latitude,
                longitude = coordinates.longitude,
                "solar provider: using configured coordinates"
            );
            coordinates
        } else if latitude.is_some() || longitude.is_some() {
            anyhow::bail!("configured coordinates require both latitude and longitude");
        } else {
            let coordinates = self.locations.resolve_coordinates().await?;
            tracing::info!(
                latitude = coordinates.latitude,
                longitude = coordinates.longitude,
                "solar provider: using provider coordinates"
            );
            coordinates
        };
        let solar_times = solar_times_for_coordinates(coordinates.latitude, coordinates.longitude)?;
        tracing::info!(
            sunrise = %solar_times.sunrise,
            sunset = %solar_times.sunset,
            "solar provider: resolved solar times"
        );
        Ok(solar_times)
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
    let coordinates = Coordinates::new(latitude, longitude)
        .ok_or_else(|| anyhow::anyhow!("invalid coordinates"))?;
    let solar_day = SolarDay::new(coordinates, date);
    let sunrise = solar_day.event_time(SolarEvent::Sunrise);
    let sunset = solar_day.event_time(SolarEvent::Sunset);

    Ok(SolarTimes {
        sunrise: sunrise.with_timezone(&Local).format("%H:%M").to_string(),
        sunset: sunset.with_timezone(&Local).format("%H:%M").to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;

    use super::{SolarTimesProvider, SolarTimesSource, solar_times_for_coordinates};
    use crate::location::provider::{CoordinatesPair, LocationProvider, LocationSource};

    struct MockSource {
        calls: Arc<AtomicUsize>,
        coordinates: CoordinatesPair,
    }

    #[async_trait]
    impl LocationSource for MockSource {
        async fn resolve_coordinates(&self) -> anyhow::Result<CoordinatesPair> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.coordinates)
        }
    }

    #[test]
    fn solar_times_format_clock_strings() {
        let times = solar_times_for_coordinates(52.2298, 21.0118).expect("solar times");
        assert_eq!(times.sunrise.len(), 5);
        assert_eq!(times.sunset.len(), 5);
        assert!(times.sunrise.contains(':'));
        assert!(times.sunset.contains(':'));
    }

    #[tokio::test]
    async fn solar_times_provider_uses_configured_coordinates_before_geoclue() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = SolarTimesProvider::with_location_provider(LocationProvider::with_source(
            Arc::new(MockSource {
                calls: calls.clone(),
                coordinates: CoordinatesPair {
                    latitude: 40.7128,
                    longitude: -74.006,
                },
            }),
        ));

        let times = provider
            .resolve_solar_times(Some(52.2298), Some(21.0118))
            .await
            .expect("solar times");

        assert_eq!(times.sunrise.len(), 5);
        assert_eq!(times.sunset.len(), 5);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn solar_times_provider_rejects_partial_configured_coordinates() {
        let calls = Arc::new(AtomicUsize::new(0));
        let provider = SolarTimesProvider::with_location_provider(LocationProvider::with_source(
            Arc::new(MockSource {
                calls: calls.clone(),
                coordinates: CoordinatesPair {
                    latitude: 40.7128,
                    longitude: -74.006,
                },
            }),
        ));

        let error = provider
            .resolve_solar_times(Some(52.2298), None)
            .await
            .expect_err("partial coordinates should fail");

        assert!(
            error
                .to_string()
                .contains("configured coordinates require both latitude and longitude")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
