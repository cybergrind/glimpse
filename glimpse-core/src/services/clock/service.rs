use std::time::Duration;

use chrono::{Local, Offset, TimeZone, Utc};
use tokio::{
    sync::{mpsc, watch},
    time::{Instant, sleep},
};
use tokio_util::sync::CancellationToken;

use crate::services::framework::{Control, ServiceCommand, ServiceHandle};

use super::model::{Command, Config, State, TimezoneConfig, WorldClockTime};

const COMMAND_QUEUE_SIZE: usize = 8;

pub type ClockHandle = ServiceHandle<State, Command>;

pub struct ClockService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

#[derive(Debug, Clone)]
struct RuntimeConfig {
    tick_interval: u64,
    timezones: Vec<RuntimeTimezone>,
}

#[derive(Debug, Clone)]
struct RuntimeTimezone {
    name: String,
    timezone: String,
    format: String,
    tz: chrono_tz::Tz,
}

impl RuntimeConfig {
    fn from_config(config: &Config) -> Self {
        Self {
            tick_interval: config.tick_interval(),
            timezones: config
                .timezones
                .iter()
                .filter_map(RuntimeTimezone::from_config)
                .collect(),
        }
    }
}

impl RuntimeTimezone {
    fn from_config(config: &TimezoneConfig) -> Option<Self> {
        let tz = match config.timezone.parse::<chrono_tz::Tz>() {
            Ok(tz) => tz,
            Err(error) => {
                tracing::warn!(timezone = %config.timezone, %error, "invalid world clock timezone");
                return None;
            }
        };

        Some(Self {
            name: if config.name.is_empty() {
                config.timezone.clone()
            } else {
                config.name.clone()
            },
            timezone: config.timezone.clone(),
            format: config.format.clone(),
            tz,
        })
    }
}

impl ClockService {
    pub fn new() -> (Self, ClockHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        let mut config = Config::default();
        let mut runtime = RuntimeConfig::from_config(&config);
        self.publish(snapshot(&runtime));
        let tick = sleep(Duration::from_secs(runtime.tick_interval));
        tokio::pin!(tick);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(Command::Configure(next))) => {
                        if config != next {
                            config = next;
                            runtime = RuntimeConfig::from_config(&config);
                            self.publish(snapshot(&runtime));
                            tick.as_mut().reset(Instant::now() + Duration::from_secs(runtime.tick_interval));
                        }
                    }
                    Some(ServiceCommand::Command(Command::Refresh)) => {
                        self.publish(snapshot(&runtime));
                    }
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => break,
                    Some(ServiceCommand::Control(Control::Start(_)))
                    | Some(ServiceCommand::Control(Control::Reconfigure(_))) => {}
                },
                _ = &mut tick => {
                    self.publish(snapshot(&runtime));
                    tick.as_mut().reset(Instant::now() + Duration::from_secs(runtime.tick_interval));
                }
            }
        }
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

fn snapshot(config: &RuntimeConfig) -> State {
    let now = rounded_local_now();
    State {
        now,
        world: config.timezones.iter().map(world_clock_time).collect(),
    }
}

fn rounded_local_now() -> chrono::DateTime<Local> {
    let now = Local::now();
    Local
        .timestamp_opt(now.timestamp(), 0)
        .single()
        .unwrap_or(now)
}

fn world_clock_time(config: &RuntimeTimezone) -> WorldClockTime {
    let local_date = Local::now().date_naive();
    let dt = Utc::now().with_timezone(&config.tz);
    let offset_secs = dt.offset().fix().local_minus_utc();
    let offset_hours = offset_secs / 3600;
    let offset_minutes = (offset_secs.abs() % 3600) / 60;
    let day_diff = dt.date_naive().signed_duration_since(local_date).num_days();

    WorldClockTime {
        name: config.name.clone(),
        timezone: config.timezone.clone(),
        time: dt.format(config.format.as_str()).to_string(),
        offset: if offset_minutes == 0 {
            format!("{offset_hours:+}")
        } else {
            format!("{offset_hours:+}:{offset_minutes:02}")
        },
        day_label: match day_diff {
            1 => Some("Tomorrow"),
            -1 => Some("Yesterday"),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_clamps_tick_interval() {
        assert_eq!(
            Config {
                tick_interval: 0,
                ..Config::default()
            }
            .tick_interval(),
            1
        );
        assert_eq!(
            Config {
                tick_interval: 120,
                ..Config::default()
            }
            .tick_interval(),
            60
        );
    }

    #[test]
    fn invalid_world_clock_timezone_is_skipped() {
        assert!(
            RuntimeTimezone::from_config(&TimezoneConfig {
                timezone: "not/a-zone".into(),
                ..TimezoneConfig::default()
            })
            .is_none()
        );
    }
}
