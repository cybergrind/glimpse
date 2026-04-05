use std::pin::Pin;
use std::process::Stdio;

use serde::Serialize;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "audio";
const TOPICS: &[&str] = &[
    "audio.status",
    "audio.outputs",
    "audio.inputs",
    "audio.streams",
];
const METHODS: &[&str] = &[
    "audio.set_volume",
    "audio.set_mute",
    "audio.set_default_output",
    "audio.set_default_input",
];

#[derive(Debug, Clone, Serialize)]
struct AudioStatus {
    default_output: String,
    default_input: String,
    volume: u32,
    muted: bool,
    icon_name: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct AudioOutput {
    index: u64,
    name: String,
    description: String,
    volume: u32,
    muted: bool,
    is_default: bool,
    icon_name: String,
}

#[derive(Debug, Clone, Serialize)]
struct AudioInput {
    index: u64,
    name: String,
    description: String,
    volume: u32,
    muted: bool,
    is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
struct AudioStream {
    index: u64,
    sink_index: u64,
    app_name: String,
    app_icon: String,
    media_name: String,
    volume: u32,
    muted: bool,
}

struct AudioProvider {
    status: AudioStatus,
    outputs: Vec<AudioOutput>,
    inputs: Vec<AudioInput>,
    streams: Vec<AudioStream>,
}

impl Provider for AudioProvider {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }

    fn run(
        &mut self,
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            tracing::info!("audio: starting");
            self.refresh().await;
            tracing::info!(
                outputs = self.outputs.len(),
                inputs = self.inputs.len(),
                streams = self.streams.len(),
                default = %self.status.default_output,
                "audio: initial state"
            );
            self.emit_all(&events).await;

            let mut subscribe = Command::new("pactl")
                .arg("subscribe")
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;

            let stdout = subscribe
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("no stdout"))?;
            let mut lines = BufReader::new(stdout).lines();

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req).await;
                    }
                    line = lines.next_line() => {
                        match line {
                            Ok(Some(line)) => {
                                if should_refresh(&line) {
                                    self.refresh().await;
                                    self.emit_all(&events).await;
                                }
                            }
                            _ => break,
                        }
                    }
                }
            }

            tracing::info!("audio: stopping");
            let _ = subscribe.kill().await;
            Ok(())
        })
    }
}

fn should_refresh(line: &str) -> bool {
    // pactl subscribe outputs lines like:
    // Event 'change' on sink #57
    // Event 'new' on sink-input #123
    line.contains("sink") || line.contains("source") || line.contains("server")
}

impl AudioProvider {
    async fn refresh(&mut self) {
        let (default_output, default_input, outputs_json, inputs_json, streams_json) = tokio::join!(
            pactl_get("get-default-sink"),
            pactl_get("get-default-source"),
            pactl_json(&["list", "sinks"]),
            pactl_json(&["list", "sources"]),
            pactl_json(&["list", "sink-inputs"]),
        );

        self.outputs = build_outputs(&default_output, &outputs_json);
        self.inputs = build_inputs(&default_input, &inputs_json);
        self.streams = build_streams(&streams_json);

        let default = self.outputs.iter().find(|o| o.is_default);
        self.status = AudioStatus {
            default_output: default.map(|o| o.description.clone()).unwrap_or_default(),
            default_input: default_input.clone(),
            volume: default.map(|o| o.volume).unwrap_or(0),
            muted: default.map(|o| o.muted).unwrap_or(false),
            icon_name: volume_icon(
                default.map(|o| o.volume).unwrap_or(0),
                default.map(|o| o.muted).unwrap_or(false),
            ),
        };
    }

    async fn emit_all(&self, events: &mpsc::Sender<ProviderEvent>) {
        let _ = events
            .send(ProviderEvent {
                topic: "audio.status".into(),
                data: serde_json::to_value(&self.status).unwrap_or_default(),
            })
            .await;
        let _ = events
            .send(ProviderEvent {
                topic: "audio.outputs".into(),
                data: serde_json::to_value(&self.outputs).unwrap_or_default(),
            })
            .await;
        let _ = events
            .send(ProviderEvent {
                topic: "audio.inputs".into(),
                data: serde_json::to_value(&self.inputs).unwrap_or_default(),
            })
            .await;
        let _ = events
            .send(ProviderEvent {
                topic: "audio.streams".into(),
                data: serde_json::to_value(&self.streams).unwrap_or_default(),
            })
            .await;
    }

    async fn handle_request(&mut self, req: ProviderRequest) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "audio.status" => serde_json::to_value(&self.status).ok(),
                    "audio.outputs" => serde_json::to_value(&self.outputs).ok(),
                    "audio.inputs" => serde_json::to_value(&self.inputs).ok(),
                    "audio.streams" => serde_json::to_value(&self.streams).ok(),
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call {
                method,
                params,
                reply,
            } => {
                let result = match method.as_str() {
                    "audio.set_volume" => {
                        tracing::info!(
                            "setting volume to {}% on {}",
                            params["volume"],
                            params["target"].as_str().unwrap_or("default")
                        );
                        cmd_set_volume(&params).await
                    }
                    "audio.set_mute" => {
                        tracing::info!(
                            "toggling mute on {}",
                            params["target"].as_str().unwrap_or("default")
                        );
                        cmd_set_mute(&params).await
                    }
                    "audio.set_default_output" => {
                        tracing::info!(
                            "setting default output to {}",
                            params["name"].as_str().unwrap_or("?")
                        );
                        cmd_set_default(&params, "set-default-sink").await
                    }
                    "audio.set_default_input" => {
                        tracing::info!(
                            "setting default input to {}",
                            params["name"].as_str().unwrap_or("?")
                        );
                        cmd_set_default(&params, "set-default-source").await
                    }
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                if let Err(ref e) = result {
                    tracing::warn!(method = %method, error = %e, "audio: call failed");
                }
                let _ = reply.send(result);
            }
        }
    }
}

/// Map pactl device.icon_name + device.form_factor to Adwaita symbolic icons.
fn resolve_device_icon(raw_icon: &str, form_factor: &str) -> String {
    // First try form_factor — most specific.
    let icon = match form_factor {
        "headset" => "audio-headset-symbolic",
        "headphone" | "headphones" => "audio-headphones-symbolic",
        "speaker" => "audio-speakers-symbolic",
        "handset" | "phone" => "phone-symbolic",
        "webcam" => "camera-web-symbolic",
        "microphone" => "audio-input-microphone-symbolic",
        _ => "",
    };
    if !icon.is_empty() {
        return icon.to_owned();
    }

    // Fall back to raw icon name mapping.
    if raw_icon.contains("headset") {
        "audio-headset-symbolic".to_owned()
    } else if raw_icon.contains("headphone") {
        "audio-headphones-symbolic".to_owned()
    } else if raw_icon.contains("hdmi") || raw_icon.contains("video") {
        "video-display-symbolic".to_owned()
    } else if raw_icon.contains("bluetooth") {
        "bluetooth-active-symbolic".to_owned()
    } else if raw_icon.contains("card") || raw_icon.contains("analog") {
        "audio-speakers-symbolic".to_owned()
    } else if raw_icon.contains("microphone") || raw_icon.contains("input") {
        "audio-input-microphone-symbolic".to_owned()
    } else {
        "audio-speakers-symbolic".to_owned()
    }
}

fn volume_icon(volume: u32, muted: bool) -> &'static str {
    if muted {
        "audio-volume-muted-symbolic"
    } else if volume == 0 {
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

async fn pactl_get(cmd: &str) -> String {
    Command::new("pactl")
        .arg(cmd)
        .output()
        .await
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

async fn pactl_json(args: &[&str]) -> serde_json::Value {
    let output = Command::new("pactl")
        .args(["--format", "json"])
        .args(args)
        .env("LC_NUMERIC", "C")
        .stderr(Stdio::null())
        .output()
        .await
        .ok();
    output
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or(json!([]))
}

fn parse_volume_percent(vol: &serde_json::Value) -> u32 {
    // Volume is {"front-left": {"value_percent": "75%"}, ...}
    // Take the first channel.
    vol.as_object()
        .and_then(|m| m.values().next())
        .and_then(|v| v["value_percent"].as_str())
        .and_then(|s| s.trim_end_matches('%').parse().ok())
        .unwrap_or(0)
}

fn build_outputs(default_name: &str, data: &serde_json::Value) -> Vec<AudioOutput> {
    let Some(arr) = data.as_array() else {
        return vec![];
    };
    arr.iter()
        .map(|s| {
            let name = s["name"].as_str().unwrap_or("").to_owned();
            let raw_icon = s["properties"]["device.icon_name"].as_str().unwrap_or("");
            let form_factor = s["properties"]["device.form_factor"].as_str().unwrap_or("");
            AudioOutput {
                index: s["index"].as_u64().unwrap_or(0),
                description: s["description"].as_str().unwrap_or("").to_owned(),
                volume: parse_volume_percent(&s["volume"]),
                muted: s["mute"].as_bool().unwrap_or(false),
                is_default: name == default_name,
                icon_name: resolve_device_icon(raw_icon, form_factor),
                name,
            }
        })
        .collect()
}

fn build_inputs(default_name: &str, data: &serde_json::Value) -> Vec<AudioInput> {
    let Some(arr) = data.as_array() else {
        return vec![];
    };
    arr.iter()
        .filter(|s| !s["name"].as_str().unwrap_or("").contains(".monitor"))
        .map(|s| {
            let name = s["name"].as_str().unwrap_or("").to_owned();
            AudioInput {
                index: s["index"].as_u64().unwrap_or(0),
                description: s["description"].as_str().unwrap_or("").to_owned(),
                volume: parse_volume_percent(&s["volume"]),
                muted: s["mute"].as_bool().unwrap_or(false),
                is_default: name == default_name,
                name,
            }
        })
        .collect()
}

fn build_streams(data: &serde_json::Value) -> Vec<AudioStream> {
    let Some(arr) = data.as_array() else {
        return vec![];
    };
    arr.iter()
        .map(|s| {
            let props = &s["properties"];
            AudioStream {
                index: s["index"].as_u64().unwrap_or(0),
                sink_index: s["sink"].as_u64().unwrap_or(0),
                app_name: props["application.name"]
                    .as_str()
                    .unwrap_or("Unknown")
                    .to_owned(),
                app_icon: props["application.icon_name"]
                    .as_str()
                    .unwrap_or("")
                    .to_owned(),
                media_name: props["media.name"].as_str().unwrap_or("").to_owned(),
                volume: parse_volume_percent(&s["volume"]),
                muted: s["mute"].as_bool().unwrap_or(false),
            }
        })
        .collect()
}

async fn cmd_set_volume(params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let target = params["target"].as_str().unwrap_or("@DEFAULT_SINK@");
    let volume = params["volume"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("missing 'volume'"))?;
    let cmd = if target.parse::<u64>().is_ok() {
        "set-sink-input-volume"
    } else if target.contains("source") || target.contains("@DEFAULT_SOURCE@") {
        "set-source-volume"
    } else {
        "set-sink-volume"
    };
    let status = Command::new("pactl")
        .args([cmd, target, &format!("{volume}%")])
        .stderr(Stdio::null())
        .status()
        .await?;
    if status.success() {
        Ok(json!(null))
    } else {
        Err(anyhow::anyhow!("pactl {cmd} failed"))
    }
}

async fn cmd_set_mute(params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let target = params["target"].as_str().unwrap_or("@DEFAULT_SINK@");
    let mute = match params.get("muted") {
        Some(serde_json::Value::Bool(true)) => "1",
        Some(serde_json::Value::Bool(false)) => "0",
        _ => "toggle",
    };
    let cmd = if target.parse::<u64>().is_ok() {
        "set-sink-input-mute"
    } else if target.contains("source") || target.contains("@DEFAULT_SOURCE@") {
        "set-source-mute"
    } else {
        "set-sink-mute"
    };
    let status = Command::new("pactl")
        .args([cmd, target, mute])
        .stderr(Stdio::null())
        .status()
        .await?;
    if status.success() {
        Ok(json!(null))
    } else {
        Err(anyhow::anyhow!("pactl {cmd} failed"))
    }
}

async fn cmd_set_default(
    params: &serde_json::Value,
    pactl_cmd: &str,
) -> anyhow::Result<serde_json::Value> {
    let name = params["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;
    let status = Command::new("pactl")
        .args([pactl_cmd, name])
        .stderr(Stdio::null())
        .status()
        .await?;
    if status.success() {
        Ok(json!(null))
    } else {
        Err(anyhow::anyhow!("pactl failed"))
    }
}

pub struct AudioProviderFactory;

impl ProviderFactory for AudioProviderFactory {
    fn name(&self) -> &'static str {
        NAME
    }
    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }
    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }
    fn create(&self) -> Box<dyn Provider> {
        Box::new(AudioProvider {
            status: AudioStatus {
                default_output: String::new(),
                default_input: String::new(),
                volume: 0,
                muted: false,
                icon_name: "audio-volume-muted-symbolic",
            },
            outputs: Vec::new(),
            inputs: Vec::new(),
            streams: Vec::new(),
        })
    }
}
