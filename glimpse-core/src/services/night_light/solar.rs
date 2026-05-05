use chrono::{Local, NaiveDate};
use sunrise::{Coordinates, SolarDay, SolarEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolarTimes {
    pub sunrise: String,
    pub sunset: String,
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
    let coordinates = Coordinates::new(coordinates.latitude, coordinates.longitude)
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

#[cfg(test)]
mod tests {
    use super::{solar_times_for_coordinates, validate_coordinates};

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
}
