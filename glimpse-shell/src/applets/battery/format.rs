use crate::services::battery::{BatteryState, BatteryStatus};

pub fn label(template: &str, status: &BatteryStatus) -> String {
    if template.is_empty() {
        return String::new();
    }

    template
        .replace("{percentage}", &status.percentage.to_string())
        .replace("{state}", state(&status.state).as_ref())
        .replace("{time_left}", &time_left(status))
        .trim_end_matches([' ', ',', '-', '—'])
        .to_owned()
}

pub fn state(state: &BatteryState) -> &'static str {
    match state {
        BatteryState::Charging => "charging",
        BatteryState::Discharging => "discharging",
        BatteryState::Empty => "empty",
        BatteryState::FullyCharged => "fully charged",
        BatteryState::PendingCharge => "pending charge",
        BatteryState::PendingDischarge => "pending discharge",
        BatteryState::Unknown => "unknown",
    }
}

pub fn time_left(status: &BatteryStatus) -> String {
    let seconds = if status.on_battery {
        status.time_to_empty
    } else {
        status.time_to_full
    };

    duration(seconds)
}

pub fn duration(seconds: i64) -> String {
    if seconds <= 0 {
        return String::new();
    }

    let minutes = seconds / 60;
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;

    if hours > 0 {
        format!("{hours}h {remaining_minutes:02}m")
    } else {
        format!("{remaining_minutes}m")
    }
}

pub fn percent(value: impl Into<f64>) -> String {
    format!("{:.0}%", value.into())
}

pub fn power_rate(watts: f64) -> String {
    format!("{watts:.1}W")
}

pub fn optional_model(model: String) -> String {
    if model.is_empty() {
        "\u{2014}".into()
    } else {
        model
    }
}

pub fn state_text(status: &BatteryStatus) -> String {
    match status.state {
        BatteryState::Discharging if status.time_to_empty > 0 => {
            format!(
                "Discharging \u{2014} {} remaining",
                duration(status.time_to_empty)
            )
        }
        BatteryState::Discharging => "Discharging".into(),
        BatteryState::Charging if status.time_to_full > 0 => {
            format!(
                "Charging \u{2014} {} until full",
                duration(status.time_to_full)
            )
        }
        BatteryState::Charging => "Charging".into(),
        BatteryState::FullyCharged => "Fully charged".into(),
        BatteryState::PendingCharge => "Plugged in, not charging".into(),
        BatteryState::PendingDischarge => "Plugged in".into(),
        BatteryState::Unknown | BatteryState::Empty => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_rounds_to_whole_percent() {
        assert_eq!(percent(73_u8), "73%");
        assert_eq!(percent(79.6), "80%");
    }

    #[test]
    fn duration_formats_with_padded_minutes() {
        assert_eq!(duration(0), "");
        assert_eq!(duration(9 * 60), "9m");
        assert_eq!(duration(65 * 60), "1h 05m");
    }
}
