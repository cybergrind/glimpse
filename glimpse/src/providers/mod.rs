pub mod audio;
pub mod battery;
pub mod bluetooth;
pub mod brightness;
pub mod calendar;
pub mod mpris;
pub mod network;
pub mod power;
pub mod power_policy;
pub mod privacy;
pub mod session_actions;
pub mod tray;

pub use crate::{calendar::CalendarServiceHandle, privacy::PrivacyServiceHandle};
