use std::process::Stdio;

use serde::Serialize;
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

const REFRESH_DEBOUNCE: Duration = Duration::from_millis(75);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AudioDevice {
    pub index: u64,
    pub name: String,
    pub description: String,
    pub volume: u32,
    pub muted: bool,
    pub is_default: bool,
    pub icon_name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AudioStream {
    pub index: u64,
    pub app_name: String,
    pub app_icon: String,
    pub volume: u32,
    pub muted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub outputs: Vec<AudioDevice>,
    pub inputs: Vec<AudioDevice>,
    pub streams: Vec<AudioStream>,
}

impl State {
    pub fn default_output(&self) -> Option<&AudioDevice> {
        self.outputs.iter().find(|device| device.is_default)
    }

    pub fn default_input(&self) -> Option<&AudioDevice> {
        self.inputs.iter().find(|device| device.is_default)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetOutputVolume(u32),
    SetInputVolume(u32),
    ToggleOutputMute,
    ToggleInputMute,
    SetDefaultOutput(String),
    SetDefaultInput(String),
    ToggleStreamMute(u64),
}

impl Command {
    fn refresh_after_execute(&self) -> bool {
        !matches!(
            self,
            Command::SetOutputVolume(_) | Command::SetInputVolume(_)
        )
    }
}

pub type AudioHandle = ServiceHandle<State, Command>;

pub struct AudioService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    events: audio_events::AudioEventsHandle,
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

struct AudioClient;

impl AudioService {
    pub fn new(events: audio_events::AudioEventsHandle) -> (Self, AudioHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(16);

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
                    tracing::warn!(%error, "audio service failed");
                    RunOutcome::RetryAfterDelay
                }
            };

            match outcome {
                RunOutcome::Cancelled => break,
                RunOutcome::RetryAfterDelay => {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        _ = sleep(Duration::from_secs(5)) => {}
                    }
                }
            }
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<RunOutcome> {
        let client = AudioClient;
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

                    if self.handle_command(command, &client).await? {
                        return Ok(RunOutcome::Cancelled);
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

    async fn handle_command(
        &mut self,
        command: ServiceCommand<Command>,
        client: &AudioClient,
    ) -> anyhow::Result<bool> {
        match command {
            ServiceCommand::Control(Control::Shutdown) => Ok(true),
            ServiceCommand::Control(Control::Start(_) | Control::Reconfigure(_)) => {
                self.refresh(client).await;
                Ok(false)
            }
            ServiceCommand::Command(command) => {
                let refresh_after_execute = command.refresh_after_execute();
                if let Err(error) = client.execute(command).await {
                    tracing::warn!(%error, "audio command failed");
                }
                if refresh_after_execute {
                    self.refresh(client).await;
                }
                Ok(false)
            }
        }
    }

    async fn refresh(&self, client: &AudioClient) -> bool {
        match client.fetch_state().await {
            Ok(state) => {
                self.change_state(state);
                true
            }
            Err(error) => {
                tracing::warn!(%error, "failed to refresh audio state");
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
            tracing::error!(?error, "failed to publish audio state");
        }
    }
}

impl AudioClient {
    async fn fetch_state(&self) -> anyhow::Result<State> {
        let info = match pactl_json(&["info"]).await {
            Ok(info) => info,
            Err(error) => {
                tracing::debug!(%error, "failed to fetch audio server info");
                serde_json::Value::Null
            }
        };
        let data = pactl_json(&["list"]).await?;
        let default_output = default_name(&info, "default_sink_name");
        let default_input = default_name(&info, "default_source_name");

        Ok(State {
            available: true,
            outputs: parse_outputs(&data["sinks"], default_output.as_deref()),
            inputs: parse_inputs(&data["sources"], default_input.as_deref()),
            streams: parse_streams(&data["sink_inputs"]),
        })
    }

    async fn execute(&self, command: Command) -> anyhow::Result<()> {
        match command {
            Command::SetOutputVolume(volume) => {
                run_pactl(&["set-sink-volume", "@DEFAULT_SINK@", &format!("{volume}%")]).await
            }
            Command::SetInputVolume(volume) => {
                run_pactl(&[
                    "set-source-volume",
                    "@DEFAULT_SOURCE@",
                    &format!("{volume}%"),
                ])
                .await
            }
            Command::ToggleOutputMute => {
                run_pactl(&["set-sink-mute", "@DEFAULT_SINK@", "toggle"]).await
            }
            Command::ToggleInputMute => {
                run_pactl(&["set-source-mute", "@DEFAULT_SOURCE@", "toggle"]).await
            }
            Command::SetDefaultOutput(name) => run_pactl(&["set-default-sink", &name]).await,
            Command::SetDefaultInput(name) => run_pactl(&["set-default-source", &name]).await,
            Command::ToggleStreamMute(stream_id) => {
                run_pactl(&["set-sink-input-mute", &stream_id.to_string(), "toggle"]).await
            }
        }
    }
}

pub fn volume_icon(volume: u32, muted: bool) -> &'static str {
    if muted || volume == 0 {
        "audio-volume-muted-symbolic"
    } else if volume < 33 {
        "audio-volume-low-symbolic"
    } else if volume < 66 {
        "audio-volume-medium-symbolic"
    } else if volume <= 100 {
        "audio-volume-high-symbolic"
    } else {
        "audio-volume-overamplified-symbolic"
    }
}

fn should_refresh(event: Option<AudioEvent>) -> bool {
    matches!(
        event,
        Some(AudioEvent::Sink | AudioEvent::Source | AudioEvent::SinkInput | AudioEvent::Server)
    )
}

async fn run_pactl(args: &[&str]) -> anyhow::Result<()> {
    let status = TokioCommand::new("pactl")
        .args(args)
        .stderr(Stdio::null())
        .status()
        .await?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("pactl {} failed", args[0]))
    }
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

fn default_name(data: &serde_json::Value, key: &str) -> Option<String> {
    data[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn parse_outputs(data: &serde_json::Value, default_name: Option<&str>) -> Vec<AudioDevice> {
    parse_devices(data, DeviceKind::Output, default_name)
}

fn parse_inputs(data: &serde_json::Value, default_name: Option<&str>) -> Vec<AudioDevice> {
    parse_devices(data, DeviceKind::Input, default_name)
}

#[derive(Clone, Copy)]
enum DeviceKind {
    Output,
    Input,
}

fn parse_devices(
    data: &serde_json::Value,
    device_kind: DeviceKind,
    default_name: Option<&str>,
) -> Vec<AudioDevice> {
    let Some(devices) = data.as_array() else {
        return Vec::new();
    };

    let mut parsed = devices
        .iter()
        .filter_map(|device| {
            let name = device["name"].as_str()?;
            if name.is_empty()
                || matches!(device_kind, DeviceKind::Input) && name.contains(".monitor")
            {
                return None;
            }

            let index = device["index"].as_u64()?;
            let description = device["description"].as_str().unwrap_or(name).to_owned();
            let form_factor = device["properties"]["device.form_factor"]
                .as_str()
                .unwrap_or("");
            let raw_icon = device["properties"]["device.icon_name"]
                .as_str()
                .unwrap_or("");

            Some(AudioDevice {
                index,
                name: name.to_owned(),
                description,
                volume: parse_volume(&device["volume"]),
                muted: device["mute"].as_bool().unwrap_or(false),
                is_default: default_name.is_some_and(|default_name| default_name == name),
                icon_name: resolve_icon(raw_icon, form_factor, device_kind),
            })
        })
        .collect::<Vec<_>>();

    if !parsed.iter().any(|device| device.is_default)
        && let Some(first) = parsed.first_mut()
    {
        first.is_default = true;
    }

    parsed
}

fn parse_streams(data: &serde_json::Value) -> Vec<AudioStream> {
    let Some(streams) = data.as_array() else {
        return Vec::new();
    };

    streams
        .iter()
        .filter_map(|stream| {
            let index = stream["index"].as_u64()?;
            let props = &stream["properties"];
            Some(AudioStream {
                index,
                app_name: props["application.name"]
                    .as_str()
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Unknown")
                    .to_owned(),
                app_icon: props["application.icon_name"]
                    .as_str()
                    .filter(|icon| !icon.is_empty())
                    .unwrap_or("application-x-executable-symbolic")
                    .to_owned(),
                volume: parse_volume(&stream["volume"]),
                muted: stream["mute"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

fn parse_volume(volume: &serde_json::Value) -> u32 {
    volume
        .as_object()
        .and_then(|channels| channels.values().next())
        .and_then(|channel| channel["value_percent"].as_str())
        .and_then(|value| value.trim_end_matches('%').parse::<u32>().ok())
        .unwrap_or(0)
}

fn resolve_icon(raw_icon: &str, form_factor: &str, device_kind: DeviceKind) -> String {
    match form_factor {
        "headset" => return "audio-headset-symbolic".into(),
        "headphone" | "headphones" => return "audio-headphones-symbolic".into(),
        "speaker" => return "audio-speakers-symbolic".into(),
        "handset" | "phone" => return "phone-symbolic".into(),
        "microphone" => return "audio-input-microphone-symbolic".into(),
        _ => {}
    }

    if raw_icon.contains("headset") {
        "audio-headset-symbolic".into()
    } else if raw_icon.contains("headphone") {
        "audio-headphones-symbolic".into()
    } else if raw_icon.contains("hdmi") || raw_icon.contains("video") {
        "video-display-symbolic".into()
    } else if raw_icon.contains("bluetooth") {
        "bluetooth-active-symbolic".into()
    } else if matches!(device_kind, DeviceKind::Input) {
        "audio-input-microphone-symbolic".into()
    } else {
        "audio-speakers-symbolic".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_icon_reflects_mute_and_volume_ranges() {
        assert_eq!(volume_icon(75, true), "audio-volume-muted-symbolic");
        assert_eq!(volume_icon(0, false), "audio-volume-muted-symbolic");
        assert_eq!(volume_icon(20, false), "audio-volume-low-symbolic");
        assert_eq!(volume_icon(50, false), "audio-volume-medium-symbolic");
        assert_eq!(volume_icon(90, false), "audio-volume-high-symbolic");
        assert_eq!(
            volume_icon(130, false),
            "audio-volume-overamplified-symbolic"
        );
    }

    #[test]
    fn parse_outputs_skips_entries_without_usable_name() {
        let data = serde_json::json!([
            {
                "index": 1,
                "name": "alsa_output.pci",
                "description": "Speakers",
                "volume": { "front-left": { "value_percent": "42%" } },
                "mute": false,
                "properties": {
                    "device.icon_name": "audio-card",
                    "device.form_factor": "speaker"
                }
            },
            {
                "index": 2,
                "description": "Broken"
            }
        ]);

        let outputs = parse_outputs(&data, None);

        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0].description, "Speakers");
        assert_eq!(outputs[0].volume, 42);
        assert!(outputs[0].is_default);
        assert_eq!(outputs[0].icon_name, "audio-speakers-symbolic");
    }

    #[test]
    fn parse_outputs_marks_reported_default_sink_even_when_not_first() {
        let data = serde_json::json!([
            {
                "index": 1,
                "name": "alsa_output.pci",
                "description": "Ryzen HD Audio Controller Analog Stereo",
                "volume": { "front-left": { "value_percent": "37%" } },
                "mute": false,
                "properties": {
                    "device.icon_name": "audio-card-analog",
                    "device.form_factor": "speaker"
                }
            },
            {
                "index": 2,
                "name": "bluez_output.F8_4E_17_BC_EE_D5.1",
                "description": "WH-1000XM4",
                "volume": { "front-left": { "value_percent": "29%" } },
                "mute": false,
                "properties": {
                    "device.icon_name": "audio-headset-bluetooth",
                    "device.form_factor": "headset"
                }
            }
        ]);

        let outputs = parse_outputs(&data, Some("bluez_output.F8_4E_17_BC_EE_D5.1"));

        assert!(!outputs[0].is_default);
        assert!(outputs[1].is_default);
        assert_eq!(outputs[1].description, "WH-1000XM4");
    }

    #[test]
    fn parse_outputs_falls_back_to_first_device_without_matching_default() {
        let data = serde_json::json!([
            {
                "index": 1,
                "name": "alsa_output.pci",
                "description": "Speakers",
                "volume": { "front-left": { "value_percent": "42%" } },
                "mute": false,
                "properties": {}
            },
            {
                "index": 2,
                "name": "bluez_output.headphones",
                "description": "Headphones",
                "volume": { "front-left": { "value_percent": "42%" } },
                "mute": false,
                "properties": {}
            }
        ]);

        let outputs = parse_outputs(&data, Some("missing"));

        assert!(outputs[0].is_default);
        assert!(!outputs[1].is_default);
    }

    #[test]
    fn default_name_trims_empty_values() {
        let data = serde_json::json!({
            "default_sink_name": " bluez_output.headphones ",
            "default_source_name": " "
        });

        assert_eq!(
            default_name(&data, "default_sink_name").as_deref(),
            Some("bluez_output.headphones")
        );
        assert_eq!(default_name(&data, "default_source_name"), None);
    }

    #[test]
    fn should_refresh_ignores_microphone_capture_events() {
        assert!(!should_refresh(Some(AudioEvent::SourceOutput)));
        assert!(should_refresh(Some(AudioEvent::Source)));
        assert!(should_refresh(Some(AudioEvent::SinkInput)));
        assert!(!should_refresh(None));
    }
}
