use std::fs;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Command;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "brightness";
const TOPICS: &[&str] = &["brightness.displays", "brightness.primary"];
const METHODS: &[&str] = &["brightness.set", "brightness.set_relative"];
const SYS_BACKLIGHT_DIR: &str = "/sys/class/backlight";
const DDC_REFRESH_SECS: u64 = 5;
const MIN_INTERNAL_BRIGHTNESS: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BacklightType {
    Firmware,
    Platform,
    Raw,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BrightnessDisplay {
    id: String,
    name: String,
    backend: String,
    current: u32,
    max: u32,
    percentage: u32,
    is_internal: bool,
    is_primary: bool,
    available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct BrightnessDisplays {
    displays: Vec<BrightnessDisplay>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct BrightnessPrimary {
    display: Option<BrightnessDisplay>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DdcVcpValue {
    current: u32,
    max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternalBacklight {
    id: String,
    name: String,
    device_name: String,
    current: u32,
    max: u32,
    backlight_type: BacklightType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DdcDisplay {
    id: String,
    name: String,
    index: u32,
    current: u32,
    max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ControlTarget {
    Internal { path: PathBuf },
    Ddc { display_index: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ControlDisplay {
    target: ControlTarget,
    data: BrightnessDisplay,
}

struct BrightnessProvider {
    displays: Vec<ControlDisplay>,
}

impl Provider for BrightnessProvider {
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
            tracing::info!("brightness: starting");
            self.refresh().await;
            self.emit_all(&events).await;

            let mut interval = tokio::time::interval(Duration::from_secs(DDC_REFRESH_SECS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = interval.tick() => {
                        if self.refresh().await {
                            self.emit_all(&events).await;
                        }
                    }
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        let changed = self.handle_request(req, &events).await;
                        if changed {
                            self.emit_all(&events).await;
                        }
                    }
                }
            }

            Ok(())
        })
    }
}

impl BrightnessProvider {
    async fn refresh(&mut self) -> bool {
        let next = discover_displays();
        if self.displays == next {
            return false;
        }
        self.displays = next;
        true
    }

    async fn emit_all(&self, events: &mpsc::Sender<ProviderEvent>) {
        let displays = BrightnessDisplays {
            displays: self.displays.iter().map(|d| d.data.clone()).collect(),
        };
        let primary = BrightnessPrimary {
            display: choose_primary_display(&displays.displays).cloned(),
        };

        let _ = events
            .send(ProviderEvent {
                topic: "brightness.displays".into(),
                data: serde_json::to_value(displays).unwrap_or_default(),
            })
            .await;
        let _ = events
            .send(ProviderEvent {
                topic: "brightness.primary".into(),
                data: serde_json::to_value(primary).unwrap_or_default(),
            })
            .await;
    }

    async fn handle_request(
        &mut self,
        req: ProviderRequest,
        events: &mpsc::Sender<ProviderEvent>,
    ) -> bool {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "brightness.displays" => Some(
                        serde_json::to_value(BrightnessDisplays {
                            displays: self.displays.iter().map(|d| d.data.clone()).collect(),
                        })
                        .unwrap_or_default(),
                    ),
                    "brightness.primary" => Some(
                        serde_json::to_value(BrightnessPrimary {
                            display: choose_primary_display(
                                &self
                                    .displays
                                    .iter()
                                    .map(|d| d.data.clone())
                                    .collect::<Vec<_>>(),
                            )
                            .cloned(),
                        })
                        .unwrap_or_default(),
                    ),
                    _ => None,
                };
                let _ = reply.send(data);
                false
            }
            ProviderRequest::Call {
                method,
                params,
                reply,
            } => {
                let result = self.handle_call(&method, params).await;
                let changed = result.is_ok() && self.refresh().await;
                if let Err(ref error) = result {
                    tracing::warn!(method = %method, %error, "brightness: call failed");
                } else if !changed {
                    // Successful writes should still update clients even when the polled state
                    // matches what we already had locally.
                    self.emit_all(events).await;
                }
                let _ = reply.send(result);
                changed
            }
        }
    }

    async fn handle_call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        match method {
            "brightness.set" => {
                let Some(display_id) = params["display_id"].as_str() else {
                    anyhow::bail!("missing 'display_id' param");
                };
                let Some(value) = params["value"].as_u64() else {
                    anyhow::bail!("missing 'value' param");
                };
                self.set_display(display_id, value as u32).await?;
                Ok(json!(null))
            }
            "brightness.set_relative" => {
                let Some(display_id) = params["display_id"].as_str() else {
                    anyhow::bail!("missing 'display_id' param");
                };
                let Some(delta) = params["delta"].as_i64() else {
                    anyhow::bail!("missing 'delta' param");
                };
                let is_percentage = params["is_percentage"].as_bool().unwrap_or(false);
                self.adjust_display(display_id, delta as i32, is_percentage)
                    .await?;
                Ok(json!(null))
            }
            _ => anyhow::bail!("unknown method: {method}"),
        }
    }

    async fn set_display(&mut self, display_id: &str, value: u32) -> anyhow::Result<()> {
        let display = self
            .displays
            .iter()
            .find(|d| d.data.id == display_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown display: {display_id}"))?;

        let clamped = clamp_brightness(value, display.data.max, display.data.is_internal);
        write_display_value(&display.target, clamped)
    }

    async fn adjust_display(
        &mut self,
        display_id: &str,
        delta: i32,
        is_percentage: bool,
    ) -> anyhow::Result<()> {
        let display = self
            .displays
            .iter()
            .find(|d| d.data.id == display_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown display: {display_id}"))?;

        let current = display.data.current as i64;
        let max = display.data.max.max(1) as i64;
        let next = if is_percentage {
            let current_pct = ((current * 100) / max) as i32;
            let next_pct = (current_pct + delta).clamp(0, 100) as i64;
            ((next_pct * max) / 100).clamp(0, max) as u32
        } else {
            (current + i64::from(delta)).clamp(0, max) as u32
        };

        self.set_display(display_id, next).await
    }
}

fn discover_displays() -> Vec<ControlDisplay> {
    let mut discovered = Vec::new();

    if let Some(internal) = discover_internal_display() {
        discovered.push(ControlDisplay {
            target: ControlTarget::Internal {
                path: Path::new(SYS_BACKLIGHT_DIR).join(&internal.device_name),
            },
            data: brightness_from_internal(internal, false),
        });
    }

    for ddc in discover_ddc_displays() {
        discovered.push(ControlDisplay {
            target: ControlTarget::Ddc {
                display_index: ddc.index,
            },
            data: brightness_from_ddc(ddc, false),
        });
    }

    let primary_id = choose_primary_display(
        &discovered
            .iter()
            .map(|display| display.data.clone())
            .collect::<Vec<_>>(),
    )
    .map(|display| display.id.clone());

    for display in &mut discovered {
        display.data.is_primary = primary_id
            .as_ref()
            .is_some_and(|primary_id| primary_id == &display.data.id);
    }

    discovered
}

fn brightness_from_internal(internal: InternalBacklight, is_primary: bool) -> BrightnessDisplay {
    BrightnessDisplay {
        id: internal.id,
        name: internal.name,
        backend: "internal".into(),
        current: internal.current,
        max: internal.max,
        percentage: percentage(internal.current, internal.max),
        is_internal: true,
        is_primary,
        available: true,
    }
}

fn brightness_from_ddc(ddc: DdcDisplay, is_primary: bool) -> BrightnessDisplay {
    BrightnessDisplay {
        id: ddc.id,
        name: ddc.name,
        backend: "ddc".into(),
        current: ddc.current,
        max: ddc.max,
        percentage: percentage(ddc.current, ddc.max),
        is_internal: false,
        is_primary,
        available: true,
    }
}

fn percentage(current: u32, max: u32) -> u32 {
    if max == 0 {
        return 0;
    }
    ((current.saturating_mul(100)) / max).min(100)
}

fn choose_primary_display(displays: &[BrightnessDisplay]) -> Option<&BrightnessDisplay> {
    displays
        .iter()
        .find(|display| display.is_internal && display.available)
        .or_else(|| displays.iter().find(|display| display.available))
}

fn select_preferred_internal(displays: &[InternalBacklight]) -> Option<&InternalBacklight> {
    displays
        .iter()
        .min_by_key(|display| match display.backlight_type {
            BacklightType::Firmware => 0,
            BacklightType::Platform => 1,
            BacklightType::Raw => 2,
        })
}

fn discover_internal_display() -> Option<InternalBacklight> {
    let entries = fs::read_dir(SYS_BACKLIGHT_DIR).ok()?;
    let mut displays = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(display) = read_internal_backlight(&path) {
            displays.push(display);
        }
    }

    select_preferred_internal(&displays).cloned()
}

fn read_internal_backlight(path: &Path) -> Option<InternalBacklight> {
    let device_name = path.file_name()?.to_str()?.to_owned();
    let current = read_u32(path.join("actual_brightness"))
        .or_else(|| read_u32(path.join("brightness")))
        .unwrap_or(0);
    let max = read_u32(path.join("max_brightness"))?;
    let backlight_type = match fs::read_to_string(path.join("type")).ok()?.trim() {
        "firmware" => BacklightType::Firmware,
        "platform" => BacklightType::Platform,
        _ => BacklightType::Raw,
    };

    Some(InternalBacklight {
        id: format!("backlight:{device_name}"),
        name: internal_display_name(&device_name),
        device_name,
        current,
        max,
        backlight_type,
    })
}

fn internal_display_name(device_name: &str) -> String {
    device_name
        .replace(['-', '_'], " ")
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn discover_ddc_displays() -> Vec<DdcDisplay> {
    let output = match Command::new("ddcutil").arg("detect").output() {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::debug!(stderr = %stderr.trim(), "brightness: ddcutil detect failed");
            return Vec::new();
        }
        Err(error) => {
            tracing::debug!(%error, "brightness: ddcutil unavailable");
            return Vec::new();
        }
    };

    parse_ddcutil_detect(&String::from_utf8_lossy(&output.stdout))
        .into_iter()
        .filter_map(|(index, name)| {
            let value = read_ddc_brightness(index)?;
            Some(DdcDisplay {
                id: format!("ddc:{index}"),
                name,
                index,
                current: value.current,
                max: value.max,
            })
        })
        .collect()
}

fn parse_ddcutil_detect(output: &str) -> Vec<(u32, String)> {
    let mut displays = Vec::new();
    let mut current_index: Option<u32> = None;
    let mut current_name: Option<String> = None;

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("Display ") {
            if let Some(index) = current_index.take() {
                displays.push((
                    index,
                    current_name
                        .take()
                        .unwrap_or_else(|| format!("Display {index}")),
                ));
            }
            current_index = rest.parse::<u32>().ok();
            current_name = None;
            continue;
        }

        if let Some(name) = line.strip_prefix("Monitor:") {
            current_name = Some(name.trim().to_owned());
        } else if let Some(connector) = line.strip_prefix("DRM connector:") {
            current_name.get_or_insert_with(|| connector.trim().to_owned());
        }
    }

    if let Some(index) = current_index {
        displays.push((
            index,
            current_name.unwrap_or_else(|| format!("Display {index}")),
        ));
    }

    displays
}

fn read_ddc_brightness(index: u32) -> Option<DdcVcpValue> {
    let output = Command::new("ddcutil")
        .args(["getvcp", "10", "--display"])
        .arg(index.to_string())
        .arg("--terse")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    parse_ddcutil_getvcp_terse(&String::from_utf8_lossy(&output.stdout))
}

fn parse_ddcutil_getvcp_terse(output: &str) -> Option<DdcVcpValue> {
    let mut parts = output.split_whitespace();
    let code = parts.next()?;
    let _kind = parts.next()?;
    let current = parts.next()?.parse().ok()?;
    let max = parts.next()?.parse().ok()?;

    if code != "10" {
        return None;
    }

    Some(DdcVcpValue { current, max })
}

fn write_display_value(target: &ControlTarget, value: u32) -> anyhow::Result<()> {
    match target {
        ControlTarget::Internal { path } => {
            fs::write(path.join("brightness"), value.to_string())?;
            Ok(())
        }
        ControlTarget::Ddc { display_index } => {
            let output = Command::new("ddcutil")
                .args(["setvcp", "10", &value.to_string(), "--display"])
                .arg(display_index.to_string())
                .output()?;
            if output.status.success() {
                Ok(())
            } else {
                anyhow::bail!(
                    "ddcutil setvcp failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )
            }
        }
    }
}

fn clamp_brightness(value: u32, max: u32, is_internal: bool) -> u32 {
    let minimum = if is_internal {
        MIN_INTERNAL_BRIGHTNESS
    } else {
        0
    };
    value.clamp(minimum.min(max), max)
}

fn read_u32(path: PathBuf) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

pub struct BrightnessProviderFactory;

impl ProviderFactory for BrightnessProviderFactory {
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
        Box::new(BrightnessProvider {
            displays: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BacklightType, BrightnessDisplay, DdcVcpValue, InternalBacklight, choose_primary_display,
        parse_ddcutil_getvcp_terse, select_preferred_internal,
    };

    #[test]
    fn select_preferred_internal_uses_firmware_priority() {
        let displays = vec![
            InternalBacklight {
                id: "raw".into(),
                name: "Raw".into(),
                device_name: "raw".into(),
                current: 20,
                max: 100,
                backlight_type: BacklightType::Raw,
            },
            InternalBacklight {
                id: "platform".into(),
                name: "Platform".into(),
                device_name: "platform".into(),
                current: 25,
                max: 100,
                backlight_type: BacklightType::Platform,
            },
            InternalBacklight {
                id: "firmware".into(),
                name: "Firmware".into(),
                device_name: "firmware".into(),
                current: 30,
                max: 100,
                backlight_type: BacklightType::Firmware,
            },
        ];

        let preferred = select_preferred_internal(&displays).expect("preferred backlight");
        assert_eq!(preferred.device_name, "firmware");
    }

    #[test]
    fn parse_ddcutil_getvcp_terse_extracts_current_and_max() {
        let parsed = parse_ddcutil_getvcp_terse("10 c 57 100").expect("parsed terse output");
        assert_eq!(
            parsed,
            DdcVcpValue {
                current: 57,
                max: 100,
            }
        );
    }

    #[test]
    fn choose_primary_display_prefers_internal_display() {
        let displays = vec![
            BrightnessDisplay {
                id: "ddc:1".into(),
                name: "Dell".into(),
                backend: "ddc".into(),
                current: 57,
                max: 100,
                percentage: 57,
                is_internal: false,
                is_primary: false,
                available: true,
            },
            BrightnessDisplay {
                id: "backlight:intel".into(),
                name: "Laptop".into(),
                backend: "internal".into(),
                current: 1200,
                max: 2000,
                percentage: 60,
                is_internal: true,
                is_primary: false,
                available: true,
            },
        ];

        let primary = choose_primary_display(&displays).expect("primary display");
        assert_eq!(primary.id, "backlight:intel");
    }
}
