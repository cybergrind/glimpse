pub(crate) mod backend;
pub mod protocol;
pub mod scheduler;
pub mod service;

pub use protocol::{
    DAYLIGHT_TEMPERATURE_KELVIN, NightLightCommand, NightLightConfig, NightLightHealth,
    NightLightPhase, NightLightSchedule, NightLightState,
};
pub use scheduler::{
    ManualScheduleWindow, ScheduleEvaluation, SolarScheduleWindow, compute_automatic_phase,
    compute_manual_phase, evaluate_automatic_schedule, evaluate_manual_schedule,
    interpolate_temperature, parse_clock_time,
};
pub use service::NightLightServiceHandle;
