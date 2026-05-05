use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::{
    process::Command as TokioCommand,
    sync::{mpsc, watch},
    time::{Duration, Instant, sleep},
};
use tokio_util::sync::CancellationToken;

use crate::services::{
    audio_events::{self, Event as AudioEvent},
    framework::{Control, ServiceCommand, ServiceHandle},
};

const COMMAND_QUEUE_SIZE: usize = 4;
const RETRY_DELAY: Duration = Duration::from_secs(5);
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(75);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MicrophoneUsage {
    pub index: u64,
    pub app_name: String,
    pub app_icon: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub usages: Vec<MicrophoneUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
}

pub type MicrophoneHandle = ServiceHandle<State, Command>;

pub struct MicrophoneService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    events: audio_events::AudioEventsHandle,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

struct MicrophoneClient;

impl MicrophoneService {
    pub fn new(events: audio_events::AudioEventsHandle) -> (Self, MicrophoneHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        (
            Self {
                state_tx,
                command_rx,
                events,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        loop {
            let outcome = match self.run_inner(cancel.clone()).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    tracing::warn!(%error, "microphone service failed");
                    RunOutcome::RetryAfterDelay
                }
            };

            match outcome {
                RunOutcome::Cancelled => break,
                RunOutcome::RetryAfterDelay => {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sleep(RETRY_DELAY) => {}
                    }
                }
            }
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<RunOutcome> {
        let client = MicrophoneClient;
        if !self.refresh(&client).await {
            return Ok(RunOutcome::RetryAfterDelay);
        }
        let mut events = self.events.subscribe();
        let refresh_timer = sleep(Duration::MAX);
        tokio::pin!(refresh_timer);
        let mut refresh_pending = false;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    return Ok(RunOutcome::Cancelled);
                }
                command = self.command_rx.recv() => {
                    let Some(command) = command else {
                        return Ok(RunOutcome::Cancelled);
                    };

                    match command {
                        ServiceCommand::Control(Control::Shutdown) => {
                            return Ok(RunOutcome::Cancelled);
                        }
                        ServiceCommand::Control(Control::Start(_) | Control::Reconfigure(_))
                        | ServiceCommand::Command(Command::Refresh) => {
                            self.refresh(&client).await;
                        }
                    }
                },
                changed = events.changed() => match changed {
                    Ok(()) => {
                        let event_state = events.borrow().clone();
                        if !event_state.available {
                            refresh_pending = false;
                            self.change_state(State::default());
                        } else if event_state.event.is_none() || should_refresh(event_state.event) {
                            if !refresh_pending {
                                refresh_pending = true;
                                refresh_timer.as_mut().reset(Instant::now() + REFRESH_DEBOUNCE);
                            }
                        }
                    }
                    Err(_) => return Ok(RunOutcome::RetryAfterDelay),
                },
                _ = &mut refresh_timer, if refresh_pending => {
                    refresh_pending = false;
                    self.refresh(&client).await;
                }
            }
        }
    }

    async fn refresh(&self, client: &MicrophoneClient) -> bool {
        match client.fetch_state().await {
            Ok(state) => {
                self.change_state(state);
                true
            }
            Err(error) => {
                tracing::warn!(%error, "failed to refresh microphone state");
                self.change_state(State::default());
                false
            }
        }
    }

    fn change_state(&self, state: State) {
        if *self.state_tx.borrow() == state {
            return;
        }

        if let Err(error) = self.state_tx.send(state) {
            tracing::error!(?error, "failed to publish microphone state");
        }
    }
}

impl MicrophoneClient {
    async fn fetch_state(&self) -> anyhow::Result<State> {
        let data = pactl_json(&["list"]).await?;
        Ok(State {
            available: true,
            usages: parse_microphone_usages(&data["source_outputs"]),
        })
    }
}

fn should_refresh(event: Option<AudioEvent>) -> bool {
    matches!(event, Some(AudioEvent::SourceOutput))
}

async fn pactl_json(args: &[&str]) -> anyhow::Result<serde_json::Value> {
    let output = TokioCommand::new("pactl")
        .args(["--format", "json"])
        .args(args)
        .env("LC_NUMERIC", "C")
        .stderr(Stdio::null())
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("pactl {} failed", args.join(" ")));
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

fn parse_microphone_usages(data: &serde_json::Value) -> Vec<MicrophoneUsage> {
    let Some(outputs) = data.as_array() else {
        return Vec::new();
    };

    let mut usages = outputs
        .iter()
        .filter_map(|output| {
            let index = output["index"].as_u64()?;
            let props = &output["properties"];
            microphone_usage_from_pactl_info(
                index,
                |key| json_string(props, key),
                output["name"].as_str(),
            )
        })
        .collect::<Vec<_>>();

    usages.sort_by(|left, right| {
        (left.app_name.as_str(), left.index).cmp(&(right.app_name.as_str(), right.index))
    });
    usages
}

fn microphone_usage_from_pactl_info(
    index: u64,
    prop: impl Fn(&str) -> Option<String>,
    stream_name: Option<&str>,
) -> Option<MicrophoneUsage> {
    let app_name =
        first_non_empty_string(&[prop("application.name"), stream_name.map(ToOwned::to_owned)])
            .unwrap_or_else(|| "Unknown".into());

    if is_ignored_microphone_client(prop("application.id").as_deref(), &app_name) {
        return None;
    }

    Some(MicrophoneUsage {
        index,
        app_name,
        app_icon: first_non_empty_string(&[prop("application.icon_name")])
            .unwrap_or_else(|| "application-x-executable-symbolic".into()),
    })
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn is_ignored_microphone_client(app_id: Option<&str>, app_name: &str) -> bool {
    const IGNORED_APP_IDS: &[&str] = &["org.gnome.VolumeControl", "org.PulseAudio.pavucontrol"];

    app_id.is_some_and(|id| IGNORED_APP_IDS.contains(&id))
        || matches!(app_name, "PulseAudio Volume Control" | "Volume Control")
}

fn first_non_empty_string(items: &[Option<String>]) -> Option<String> {
    items
        .iter()
        .flatten()
        .find(|item| !item.is_empty())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_recording_apps_from_source_outputs() {
        let usage = microphone_usage_from_pactl_info(
            7,
            |key| match key {
                "application.name" => Some("Telegram".into()),
                "application.icon_name" => Some("telegram".into()),
                _ => None,
            },
            None,
        );

        assert_eq!(
            usage,
            Some(MicrophoneUsage {
                index: 7,
                app_name: "Telegram".into(),
                app_icon: "telegram".into(),
            })
        );
    }

    #[test]
    fn parses_source_outputs_from_pactl_json() {
        let usages = parse_microphone_usages(&serde_json::json!([
            {
                "index": 8,
                "name": "record stream",
                "properties": {
                    "application.name": "Firefox",
                    "application.icon_name": "firefox"
                }
            }
        ]));

        assert_eq!(
            usages,
            vec![MicrophoneUsage {
                index: 8,
                app_name: "Firefox".into(),
                app_icon: "firefox".into(),
            }]
        );
    }

    #[test]
    fn ignores_volume_control_source_outputs() {
        assert_eq!(
            microphone_usage_from_pactl_info(
                8,
                |key| match key {
                    "application.id" => Some("org.PulseAudio.pavucontrol".into()),
                    "application.name" => Some("PulseAudio Volume Control".into()),
                    _ => None,
                },
                None,
            ),
            None
        );
    }

    #[test]
    fn should_refresh_only_for_source_output_events() {
        assert!(should_refresh(Some(AudioEvent::SourceOutput)));
        assert!(!should_refresh(Some(AudioEvent::Source)));
        assert!(!should_refresh(Some(AudioEvent::Sink)));
        assert!(!should_refresh(None));
    }
}
