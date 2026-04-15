use crate::night_light::protocol::NightLightPhase;

const MINUTES_PER_DAY: u16 = 24 * 60;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScheduleEvaluation {
    pub phase: NightLightPhase,
    pub night_progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManualScheduleWindow {
    start_minutes: u16,
    end_minutes: u16,
    transition_minutes: u16,
}

impl ManualScheduleWindow {
    pub fn new(start: &str, end: &str, transition_minutes: u32) -> Result<Self, String> {
        let start_minutes = parse_clock_time(start)?;
        let end_minutes = parse_clock_time(end)?;
        let transition_minutes = u16::try_from(transition_minutes)
            .map_err(|_| "transition_minutes does not fit in u16".to_string())?;

        Ok(Self {
            start_minutes,
            end_minutes,
            transition_minutes,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolarScheduleWindow {
    sunset_minutes: u16,
    sunrise_minutes: u16,
    transition_minutes: u16,
}

impl SolarScheduleWindow {
    pub fn new(sunset: &str, sunrise: &str, transition_minutes: u32) -> Result<Self, String> {
        let sunset_minutes = parse_clock_time(sunset)?;
        let sunrise_minutes = parse_clock_time(sunrise)?;
        let transition_minutes = u16::try_from(transition_minutes)
            .map_err(|_| "transition_minutes does not fit in u16".to_string())?;

        Ok(Self {
            sunset_minutes,
            sunrise_minutes,
            transition_minutes,
        })
    }
}

pub fn parse_clock_time(value: &str) -> Result<u16, String> {
    let (hours, minutes) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid clock time `{value}`"))?;
    let hours: u16 = hours
        .parse()
        .map_err(|_| format!("invalid hour in `{value}`"))?;
    let minutes: u16 = minutes
        .parse()
        .map_err(|_| format!("invalid minute in `{value}`"))?;

    if hours >= 24 || minutes >= 60 {
        return Err(format!("clock time `{value}` is out of range"));
    }

    Ok(hours * 60 + minutes)
}

pub fn compute_manual_phase(
    window: &ManualScheduleWindow,
    current_time: &str,
) -> Result<NightLightPhase, String> {
    Ok(evaluate_manual_schedule(window, current_time)?.phase)
}

pub fn evaluate_manual_schedule(
    window: &ManualScheduleWindow,
    current_time: &str,
) -> Result<ScheduleEvaluation, String> {
    let current_minutes = parse_clock_time(current_time)?;
    Ok(evaluate_window(
        window.start_minutes,
        window.end_minutes,
        window.transition_minutes,
        current_minutes,
    ))
}

pub fn compute_automatic_phase(
    window: &SolarScheduleWindow,
    current_time: &str,
) -> Result<NightLightPhase, String> {
    Ok(evaluate_automatic_schedule(window, current_time)?.phase)
}

pub fn evaluate_automatic_schedule(
    window: &SolarScheduleWindow,
    current_time: &str,
) -> Result<ScheduleEvaluation, String> {
    let current_minutes = parse_clock_time(current_time)?;
    Ok(evaluate_window(
        window.sunset_minutes,
        window.sunrise_minutes,
        window.transition_minutes,
        current_minutes,
    ))
}

pub fn interpolate_temperature(day_temperature: u32, night_temperature: u32, progress: f32) -> u32 {
    let progress = progress.clamp(0.0, 1.0) as f64;
    let day_temperature = day_temperature as f64;
    let night_temperature = night_temperature as f64;
    (day_temperature + (night_temperature - day_temperature) * progress).round() as u32
}

fn evaluate_window(start: u16, end: u16, transition: u16, current: u16) -> ScheduleEvaluation {
    if start == end {
        return ScheduleEvaluation {
            phase: NightLightPhase::Night,
            night_progress: 1.0,
        };
    }

    if transition > 0 {
        let elapsed_from_start = elapsed_minutes(start, current);
        if elapsed_from_start < transition {
            return ScheduleEvaluation {
                phase: NightLightPhase::TransitionToNight,
                night_progress: elapsed_from_start as f32 / transition as f32,
            };
        }

        let elapsed_from_end = elapsed_minutes(end, current);
        if elapsed_from_end < transition {
            return ScheduleEvaluation {
                phase: NightLightPhase::TransitionToDay,
                night_progress: 1.0 - (elapsed_from_end as f32 / transition as f32),
            };
        }
    }

    if contains_time(start, end, current) {
        ScheduleEvaluation {
            phase: NightLightPhase::Night,
            night_progress: 1.0,
        }
    } else {
        ScheduleEvaluation {
            phase: NightLightPhase::Day,
            night_progress: 0.0,
        }
    }
}

fn contains_time(start: u16, end: u16, current: u16) -> bool {
    if start < end {
        current >= start && current < end
    } else {
        current >= start || current < end
    }
}
fn elapsed_minutes(start: u16, end: u16) -> u16 {
    if end >= start {
        end - start
    } else {
        end + MINUTES_PER_DAY - start
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ManualScheduleWindow, SolarScheduleWindow, compute_automatic_phase, compute_manual_phase,
        evaluate_automatic_schedule, evaluate_manual_schedule, interpolate_temperature,
    };
    use crate::night_light::protocol::NightLightPhase;

    #[test]
    fn manual_schedule_supports_overnight_windows() {
        let window = ManualScheduleWindow::new("22:00", "06:00", 90).expect("window");
        let phase = compute_manual_phase(&window, "23:30").expect("phase");
        assert_eq!(phase, NightLightPhase::Night);
    }

    #[test]
    fn interpolation_reaches_midpoint_during_transition() {
        let current = interpolate_temperature(6500, 4500, 0.5);
        assert_eq!(current, 5500);
    }

    #[test]
    fn manual_schedule_reports_transition_progress() {
        let window = ManualScheduleWindow::new("18:00", "06:00", 90).expect("window");
        let evaluation = evaluate_manual_schedule(&window, "18:45").expect("evaluation");
        assert_eq!(evaluation.phase, NightLightPhase::TransitionToNight);
        assert_eq!(evaluation.night_progress, 0.5);
    }

    #[test]
    fn automatic_schedule_uses_day_between_sunrise_and_sunset() {
        let window = SolarScheduleWindow::new("18:00", "06:00", 90).expect("window");
        let phase = compute_automatic_phase(&window, "12:00").expect("phase");
        assert_eq!(phase, NightLightPhase::Day);
    }

    #[test]
    fn automatic_schedule_reports_morning_transition() {
        let window = SolarScheduleWindow::new("18:00", "06:00", 90).expect("window");
        let evaluation = evaluate_automatic_schedule(&window, "06:45").expect("evaluation");
        assert_eq!(evaluation.phase, NightLightPhase::TransitionToDay);
        assert_eq!(evaluation.night_progress, 0.5);
    }
}
