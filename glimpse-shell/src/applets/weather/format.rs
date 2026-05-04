use glimpse_core::services::weather::model::{CurrentWeather, Location, State};

pub const DEFAULT_LABEL_FORMAT: &str = "{temp}°";
pub const DEFAULT_TOOLTIP_FORMAT: &str =
    "{condition} · {temp} · feels like {feels_like} · {location}";

pub fn icon_name(state: &State) -> String {
    match state {
        State::Ready(snapshot) => snapshot.current.icon.clone(),
        State::Loading => "weather-overcast-symbolic".into(),
        State::Unknown | State::Unavailable(_) => "weather-overcast-symbolic".into(),
    }
}

pub fn label(template: &str, state: &State) -> String {
    let State::Ready(snapshot) = state else {
        return String::new();
    };

    text(template, &snapshot.current, &snapshot.location)
}

pub fn tooltip(template: &str, state: &State) -> String {
    match state {
        State::Ready(snapshot) => text(template, &snapshot.current, &snapshot.location),
        State::Loading => "Loading weather".into(),
        State::Unavailable(message) if !message.is_empty() => message.clone(),
        State::Unknown | State::Unavailable(_) => "Weather".into(),
    }
}

pub fn text(template: &str, current: &CurrentWeather, location: &Location) -> String {
    template
        .replace("{temp}", &format!("{:.0}°", current.temperature))
        .replace("{condition}", &current.condition)
        .replace(
            "{feels_like}",
            &format!("{:.0}°", current.apparent_temperature),
        )
        .replace("{location}", &location.city)
}

pub fn temperature(value: f64) -> String {
    format!("{value:.0}°")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_replaces_weather_placeholders() {
        let current = CurrentWeather {
            temperature: 19.6,
            apparent_temperature: 18.2,
            condition: "Cloudy".into(),
            ..CurrentWeather::default()
        };
        let location = Location {
            city: "Warsaw, PL".into(),
            ..Location::default()
        };

        assert_eq!(
            text(
                "{condition} · {temp} · {feels_like} · {location}",
                &current,
                &location,
            ),
            "Cloudy · 20° · 18° · Warsaw, PL"
        );
    }
}
