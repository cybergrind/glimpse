use std::process::Stdio;

use serde::Serialize;
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AudioDevice {
    pub index: u64,
    pub name: String,
    pub description: String,
    pub volume: u32,
    pub muted: bool,
    pub is_default: bool,
    pub icon_name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AudioStream {
    pub index: u64,
    pub app_name: String,
    pub app_icon: String,
    pub volume: u32,
    pub muted: bool,
}

#[derive(Debug, Clone)]
pub struct DeviceList(Vec<AudioDevice>);

impl DeviceList {
    pub fn default_device(&self) -> Option<&AudioDevice> {
        self.0.iter().find(|d| d.is_default)
    }
}

impl std::ops::Deref for DeviceList {
    type Target = Vec<AudioDevice>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub enum AudioEvent {
    OutputsChanged(DeviceList),
    InputsChanged(DeviceList),
    StreamsChanged(Vec<AudioStream>),
    Unavailable,
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

pub struct AudioProvider;

impl AudioProvider {
    pub fn new() -> Self {
        Self
    }

    pub async fn run(
        &mut self,
        tx: mpsc::Sender<AudioEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        if Command::new("pactl")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_err()
        {
            tracing::warn!("audio: pactl not found, audio disabled");
            let _ = tx.send(AudioEvent::Unavailable).await;
            return Ok(());
        }

        let (mut prev_outputs, mut prev_inputs, mut prev_streams) = self.fetch_all().await;

        tracing::info!(
            outputs = prev_outputs.len(),
            inputs = prev_inputs.len(),
            streams = prev_streams.len(),
            "audio: initial state"
        );

        let _ = tx
            .send(AudioEvent::OutputsChanged(DeviceList(prev_outputs.clone())))
            .await;
        let _ = tx
            .send(AudioEvent::InputsChanged(DeviceList(prev_inputs.clone())))
            .await;
        let _ = tx
            .send(AudioEvent::StreamsChanged(prev_streams.clone()))
            .await;

        let mut sub = Command::new("pactl")
            .arg("subscribe")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdout = sub
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("no stdout"))?;
        let mut lines = tokio::io::BufReader::new(stdout).lines();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                line = lines.next_line() => {
                    match line {
                        Ok(Some(line)) if should_refresh(&line) => {
                            let (outputs, inputs, streams) = self.fetch_all().await;
                            if outputs != prev_outputs {
                                prev_outputs = outputs.clone();
                                let _ = tx.send(AudioEvent::OutputsChanged(DeviceList(outputs))).await;
                            }
                            if inputs != prev_inputs {
                                prev_inputs = inputs.clone();
                                let _ = tx.send(AudioEvent::InputsChanged(DeviceList(inputs))).await;
                            }
                            if streams != prev_streams {
                                prev_streams = streams.clone();
                                let _ = tx.send(AudioEvent::StreamsChanged(streams)).await;
                            }
                        }
                        Ok(Some(_)) => {}
                        _ => break,
                    }
                }
            }
        }

        let _ = sub.kill().await;
        Ok(())
    }

    pub async fn set_volume(&self, target: &str, volume: u32) -> anyhow::Result<()> {
        let cmd = if target.parse::<u64>().is_ok() {
            "set-sink-input-volume"
        } else if target.contains("source") || target == "@DEFAULT_SOURCE@" {
            "set-source-volume"
        } else {
            "set-sink-volume"
        };
        run_pactl(&[cmd, target, &format!("{volume}%")]).await
    }

    pub async fn toggle_mute(&self, target: &str) -> anyhow::Result<()> {
        self.set_mute_inner(target, "toggle").await
    }

    pub async fn set_mute(&self, target: &str, muted: bool) -> anyhow::Result<()> {
        self.set_mute_inner(target, if muted { "1" } else { "0" })
            .await
    }

    pub async fn set_default_output(&self, name: &str) -> anyhow::Result<()> {
        run_pactl(&["set-default-sink", name]).await
    }

    pub async fn set_default_input(&self, name: &str) -> anyhow::Result<()> {
        run_pactl(&["set-default-source", name]).await
    }

    async fn set_mute_inner(&self, target: &str, value: &str) -> anyhow::Result<()> {
        let cmd = if target.parse::<u64>().is_ok() {
            "set-sink-input-mute"
        } else if target.contains("source") || target == "@DEFAULT_SOURCE@" {
            "set-source-mute"
        } else {
            "set-sink-mute"
        };
        run_pactl(&[cmd, target, value]).await
    }

    async fn fetch_all(&self) -> (Vec<AudioDevice>, Vec<AudioDevice>, Vec<AudioStream>) {
        let (default_sink, default_source, sinks_json, sources_json, inputs_json) = tokio::join!(
            pactl_text("get-default-sink"),
            pactl_text("get-default-source"),
            pactl_json(&["list", "sinks"]),
            pactl_json(&["list", "sources"]),
            pactl_json(&["list", "sink-inputs"]),
        );
        let outputs = parse_outputs(&default_sink, &sinks_json);
        let inputs = parse_inputs(&default_source, &sources_json);
        let streams = parse_streams(&inputs_json);
        (outputs, inputs, streams)
    }
}

fn should_refresh(line: &str) -> bool {
    line.contains("sink") || line.contains("source") || line.contains("server")
}

async fn run_pactl(args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("pactl")
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

async fn pactl_text(arg: &str) -> String {
    Command::new("pactl")
        .arg(arg)
        .output()
        .await
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

async fn pactl_json(args: &[&str]) -> serde_json::Value {
    Command::new("pactl")
        .args(["--format", "json"])
        .args(args)
        .env("LC_NUMERIC", "C")
        .stderr(Stdio::null())
        .output()
        .await
        .ok()
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or(serde_json::json!([]))
}

fn parse_volume(vol: &serde_json::Value) -> u32 {
    vol.as_object()
        .and_then(|m| m.values().next())
        .and_then(|v| v["value_percent"].as_str())
        .and_then(|s| s.trim_end_matches('%').parse().ok())
        .unwrap_or(0)
}

fn resolve_icon(raw_icon: &str, form_factor: &str) -> String {
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
    } else {
        "audio-speakers-symbolic".into()
    }
}

fn parse_outputs(default_name: &str, data: &serde_json::Value) -> Vec<AudioDevice> {
    let Some(arr) = data.as_array() else {
        return vec![];
    };
    arr.iter()
        .map(|s| {
            let name = s["name"].as_str().unwrap_or("").to_owned();
            AudioDevice {
                index: s["index"].as_u64().unwrap_or(0),
                description: s["description"].as_str().unwrap_or("").to_owned(),
                volume: parse_volume(&s["volume"]),
                muted: s["mute"].as_bool().unwrap_or(false),
                is_default: name == default_name,
                icon_name: resolve_icon(
                    s["properties"]["device.icon_name"].as_str().unwrap_or(""),
                    s["properties"]["device.form_factor"].as_str().unwrap_or(""),
                ),
                name,
            }
        })
        .collect()
}

fn parse_inputs(default_name: &str, data: &serde_json::Value) -> Vec<AudioDevice> {
    let Some(arr) = data.as_array() else {
        return vec![];
    };
    arr.iter()
        .filter(|s| !s["name"].as_str().unwrap_or("").contains(".monitor"))
        .map(|s| {
            let name = s["name"].as_str().unwrap_or("").to_owned();
            AudioDevice {
                index: s["index"].as_u64().unwrap_or(0),
                description: s["description"].as_str().unwrap_or("").to_owned(),
                volume: parse_volume(&s["volume"]),
                muted: s["mute"].as_bool().unwrap_or(false),
                is_default: name == default_name,
                icon_name: resolve_icon(
                    s["properties"]["device.icon_name"].as_str().unwrap_or(""),
                    s["properties"]["device.form_factor"].as_str().unwrap_or(""),
                ),
                name,
            }
        })
        .collect()
}

fn parse_streams(data: &serde_json::Value) -> Vec<AudioStream> {
    let Some(arr) = data.as_array() else {
        return vec![];
    };
    arr.iter()
        .map(|s| {
            let props = &s["properties"];
            AudioStream {
                index: s["index"].as_u64().unwrap_or(0),
                app_name: props["application.name"]
                    .as_str()
                    .unwrap_or("Unknown")
                    .to_owned(),
                app_icon: props["application.icon_name"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
                volume: parse_volume(&s["volume"]),
                muted: s["mute"].as_bool().unwrap_or(false),
            }
        })
        .collect()
}
