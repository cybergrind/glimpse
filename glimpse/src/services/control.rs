use crate::config::Config;

pub enum ControlEvent {
    Configure(Config),
    Shutdown,
}
