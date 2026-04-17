use crate::config::Config;

pub enum ControlEvent {
    Reconfigure(Config),
}
