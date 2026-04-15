use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use serde::Serialize;
use tokio::sync::{Mutex, mpsc};
use tokio_util::sync::CancellationToken;

use crate::dbus::login1::{Login1ManagerProxy, Login1SessionProxy};
use crate::display::{
    DisplayConnectorKind, DrmConnectorState, connector_kind, drm_connector_states,
    normalize_connector_name,
};

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const SYS_BACKLIGHT_DIR: &str = "/sys/class/backlight";
const MIN_INTERNAL_BRIGHTNESS: u32 = 1;
const DDC_WRITE_ATTEMPTS: usize = 3;
const DDC_WRITE_RETRY_DELAY: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BrightnessBackend {
    Backlight,
    Ddc,
}

impl fmt::Display for BrightnessBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backlight => f.write_str("backlight"),
            Self::Ddc => f.write_str("ddc"),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BrightnessDisplay {
    pub id: String,
    pub name: String,
    pub backend: BrightnessBackend,
    pub current: u32,
    pub max: u32,
    pub percentage: u8,
    pub is_internal: bool,
    pub is_primary: bool,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct BrightnessStatus {
    pub available: bool,
    pub display_count: u32,
    pub primary_display_id: Option<String>,
}

impl BrightnessStatus {
    fn from_displays(displays: &[BrightnessDisplay]) -> Self {
        Self {
            available: displays.iter().any(|display| display.available),
            display_count: displays.len() as u32,
            primary_display_id: choose_primary_display(displays).map(|display| display.id.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct BrightnessSnapshot {
    pub status: BrightnessStatus,
    pub displays: Vec<BrightnessDisplay>,
}

impl BrightnessSnapshot {
    fn new(mut displays: Vec<BrightnessDisplay>) -> Self {
        displays.sort_by(|left, right| {
            right
                .is_primary
                .cmp(&left.is_primary)
                .then(right.is_internal.cmp(&left.is_internal))
                .then(left.name.cmp(&right.name))
        });

        Self {
            status: BrightnessStatus::from_displays(&displays),
            displays,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrightnessChangeReason {
    DisplaysChanged,
    LevelsChanged,
    AvailabilityChanged,
    Mixed,
}

impl fmt::Display for BrightnessChangeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DisplaysChanged => f.write_str("displays-changed"),
            Self::LevelsChanged => f.write_str("levels-changed"),
            Self::AvailabilityChanged => f.write_str("availability-changed"),
            Self::Mixed => f.write_str("mixed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrightnessProviderEvent {
    Changed { reason: BrightnessChangeReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum BacklightType {
    Firmware,
    Platform,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DdcVcpValue {
    current: u32,
    max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternalBacklight {
    id: String,
    name: String,
    connector: Option<String>,
    device_name: String,
    current: u32,
    max: u32,
    backlight_type: BacklightType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DdcDisplay {
    id: String,
    name: String,
    connector: Option<String>,
    index: u32,
    current: u32,
    max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ControlTarget {
    Internal(InternalControlTarget),
    Ddc(ExternalControlTarget),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ControlDisplay {
    target: ControlTarget,
    data: BrightnessDisplay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternalControlTarget {
    path: PathBuf,
    device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExternalControlTarget {
    display_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Login1BrightnessSession {
    path: zbus::zvariant::OwnedObjectPath,
    user_name: String,
    seat: String,
    class: String,
    kind: String,
    active: bool,
}

impl Login1BrightnessSession {
    fn supports_brightness(&self) -> bool {
        self.active
            && self.class == "user"
            && !self.seat.is_empty()
            && !self.kind.is_empty()
            && self.kind != "unspecified"
    }
}

#[derive(Default)]
struct ProviderState {
    displays: Vec<ControlDisplay>,
    detected_ddc_displays: Vec<DetectedDdcDisplay>,
    connector_fingerprint: Vec<String>,
}

#[derive(Clone)]
struct InternalBrightnessBackend {
    conn: zbus::Connection,
}

impl InternalBrightnessBackend {
    fn new(conn: zbus::Connection) -> Self {
        Self { conn }
    }

    fn discover_display(&self, connectors: &[DrmConnectorState]) -> Option<ControlDisplay> {
        let internal = discover_internal_backlight(connectors)?;
        let available = display_availability(internal.connector.as_deref(), true, connectors);
        let target = ControlTarget::Internal(InternalControlTarget {
            path: Path::new(SYS_BACKLIGHT_DIR).join(&internal.device_name),
            device_name: internal.device_name.clone(),
        });

        Some(ControlDisplay {
            target,
            data: brightness_from_internal(internal, false, available),
        })
    }

    async fn write_value(&self, target: &InternalControlTarget, value: u32) -> anyhow::Result<()> {
        if let Err(error) = self
            .write_via_logind(&target.device_name, value)
            .await
            .with_context(|| {
                format!(
                    "failed to set brightness via logind for {}",
                    target.device_name
                )
            })
        {
            tracing::debug!(
                error = %error,
                device_name = target.device_name,
                "brightness provider: falling back to sysfs write"
            );
            fs::write(target.path.join("brightness"), value.to_string()).with_context(|| {
                format!(
                    "failed sysfs fallback for {} after logind error: {error}",
                    target.device_name
                )
            })?;
        }

        Ok(())
    }

    async fn write_via_logind(&self, device_name: &str, value: u32) -> anyhow::Result<()> {
        let manager = Login1ManagerProxy::new(&self.conn).await?;
        let session_path = self.resolve_brightness_session_path(&manager).await?;
        let session = Login1SessionProxy::builder(&self.conn)
            .path(session_path)?
            .build()
            .await?;
        session
            .set_brightness("backlight", device_name, value)
            .await?;
        Ok(())
    }

    async fn resolve_brightness_session_path(
        &self,
        manager: &Login1ManagerProxy<'_>,
    ) -> anyhow::Result<zbus::zvariant::OwnedObjectPath> {
        let current_session = match manager.get_session_by_pid(std::process::id()).await {
            Ok(path) => Some(self.load_session(path).await?),
            Err(error) => {
                tracing::debug!(
                    error = %error,
                    "brightness provider: failed to resolve current login1 session"
                );
                None
            }
        };
        let current_user = env::var("USER").ok();
        let mut listed_sessions = Vec::new();

        for (_id, _uid, user_name, seat, path) in manager.list_sessions().await? {
            if seat.is_empty() {
                continue;
            }

            match self.load_listed_session((user_name, seat, path)).await {
                Ok(session) => listed_sessions.push(session),
                Err(error) => {
                    tracing::debug!(
                        error = %error,
                        "brightness provider: failed to inspect login1 session"
                    );
                }
            }
        }

        choose_brightness_session(
            current_session.as_ref(),
            &listed_sessions,
            current_user.as_deref(),
        )
        .map(|session| session.path.clone())
        .ok_or_else(|| anyhow::anyhow!("no active login1 brightness session available"))
    }

    async fn load_session(
        &self,
        path: zbus::zvariant::OwnedObjectPath,
    ) -> anyhow::Result<Login1BrightnessSession> {
        let session = Login1SessionProxy::builder(&self.conn)
            .path(path.clone())?
            .build()
            .await?;
        let (seat, _) = session.seat().await?;

        Ok(Login1BrightnessSession {
            path,
            user_name: session.name().await?,
            seat,
            class: session.class().await?,
            kind: session.kind().await?,
            active: session.active().await?,
        })
    }

    async fn load_listed_session(
        &self,
        session: (String, String, zbus::zvariant::OwnedObjectPath),
    ) -> anyhow::Result<Login1BrightnessSession> {
        let (user_name, seat, path) = session;
        let proxy = Login1SessionProxy::builder(&self.conn)
            .path(path.clone())?
            .build()
            .await?;

        Ok(Login1BrightnessSession {
            path,
            user_name,
            seat,
            class: proxy.class().await?,
            kind: proxy.kind().await?,
            active: proxy.active().await?,
        })
    }
}

#[derive(Clone, Default)]
struct ExternalBrightnessBackend;

impl ExternalBrightnessBackend {
    fn discover_displays(
        &self,
        detected_displays: &[DetectedDdcDisplay],
        connectors: &[DrmConnectorState],
    ) -> Vec<ControlDisplay> {
        detected_displays
            .iter()
            .cloned()
            .into_iter()
            .filter_map(|detected| {
                let value = self.read_brightness(detected.index)?;
                let display = ddc_display_from_detected(detected, value, connectors)?;
                let is_internal = display.connector.as_deref().is_some_and(|connector| {
                    connector_kind(connector) == DisplayConnectorKind::Internal
                });
                let available =
                    display_availability(display.connector.as_deref(), is_internal, connectors);

                Some(ControlDisplay {
                    target: ControlTarget::Ddc(ExternalControlTarget {
                        display_index: display.index,
                    }),
                    data: brightness_from_ddc(display, false, is_internal, available),
                })
            })
            .collect()
    }

    async fn write_value(&self, target: &ExternalControlTarget, value: u32) -> anyhow::Result<()> {
        let display_index = target.display_index;
        tokio::task::spawn_blocking(move || write_ddc_value(display_index, value))
            .await
            .map_err(|error| anyhow::anyhow!("ddcutil worker failed: {error}"))?
    }

    fn detect_displays(&self) -> Vec<DetectedDdcDisplay> {
        let output = match Command::new("ddcutil").arg("detect").output() {
            Ok(output) if output.status.success() => output,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::debug!(
                    stderr = %stderr.trim(),
                    "brightness provider: ddcutil detect failed"
                );
                return Vec::new();
            }
            Err(error) => {
                tracing::debug!(%error, "brightness provider: ddcutil unavailable");
                return Vec::new();
            }
        };

        parse_ddcutil_detect(&String::from_utf8_lossy(&output.stdout))
    }

    fn read_brightness(&self, index: u32) -> Option<DdcVcpValue> {
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
}

#[derive(Clone)]
pub struct BrightnessProvider {
    internal: InternalBrightnessBackend,
    external: ExternalBrightnessBackend,
    state: Arc<Mutex<ProviderState>>,
}

impl BrightnessProvider {
    pub fn new(conn: zbus::Connection) -> Self {
        Self {
            internal: InternalBrightnessBackend::new(conn),
            external: ExternalBrightnessBackend,
            state: Arc::new(Mutex::new(ProviderState::default())),
        }
    }

    pub async fn snapshot(&self) -> anyhow::Result<BrightnessSnapshot> {
        let mut state = self.state.lock().await;
        state.displays = self.discover_displays(&mut state);
        Ok(BrightnessSnapshot::new(
            state
                .displays
                .iter()
                .map(|display| display.data.clone())
                .collect(),
        ))
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<BrightnessProviderEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let mut interval = tokio::time::interval(POLL_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut previous = self.snapshot().await?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = interval.tick() => {
                    let snapshot = self.snapshot().await?;
                    if snapshot != previous {
                        let reason = classify_change(&previous, &snapshot);
                        previous = snapshot;
                        if events.send(BrightnessProviderEvent::Changed { reason }).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn adjust_display_percent(
        &self,
        display_id: &str,
        delta_percent: i32,
    ) -> anyhow::Result<()> {
        let display = self.lookup_display(display_id).await?;
        let next_percent = (i32::from(display.data.percentage) + delta_percent).clamp(0, 100) as u8;
        let value = raw_value_for_percent(next_percent, display.data.max, display.data.is_internal);
        self.write_display_value(&display.target, value).await
    }

    pub async fn set_display_percent(&self, display_id: &str, percent: u8) -> anyhow::Result<()> {
        let display = self.lookup_display(display_id).await?;
        let value = raw_value_for_percent(percent, display.data.max, display.data.is_internal);
        self.write_display_value(&display.target, value).await
    }

    pub async fn set_primary_display_percent(&self, percent: u8) -> anyhow::Result<()> {
        let display = self.lookup_primary_display().await?;
        let value = raw_value_for_percent(percent, display.data.max, display.data.is_internal);
        self.write_display_value(&display.target, value).await
    }

    fn discover_displays(&self, state: &mut ProviderState) -> Vec<ControlDisplay> {
        let connectors = drm_connector_states();
        let connector_fingerprint = connected_connector_fingerprint(&connectors);
        if state.detected_ddc_displays.is_empty()
            || state.connector_fingerprint != connector_fingerprint
        {
            state.detected_ddc_displays = self.external.detect_displays();
            state.connector_fingerprint = connector_fingerprint;
        }
        let mut discovered = Vec::new();

        if let Some(display) = self.internal.discover_display(&connectors) {
            discovered.push(display);
        }

        discovered.extend(
            self.external
                .discover_displays(&state.detected_ddc_displays, &connectors),
        );
        let mut discovered = filter_available_displays(discovered);

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

    async fn lookup_display(&self, display_id: &str) -> anyhow::Result<ControlDisplay> {
        let mut state = self.state.lock().await;
        if state.displays.is_empty() {
            state.displays = self.discover_displays(&mut state);
        }

        if let Some(display) = lookup_cached_display(&state.displays, display_id) {
            return Ok(display);
        }

        state.displays = self.discover_displays(&mut state);
        lookup_cached_display(&state.displays, display_id)
            .ok_or_else(|| anyhow::anyhow!("unknown display: {display_id}"))
    }

    async fn lookup_primary_display(&self) -> anyhow::Result<ControlDisplay> {
        let mut state = self.state.lock().await;
        if state.displays.is_empty() {
            state.displays = self.discover_displays(&mut state);
        }

        if let Some(display) = lookup_cached_primary_display(&state.displays) {
            return Ok(display);
        }

        state.displays = self.discover_displays(&mut state);
        lookup_cached_primary_display(&state.displays)
            .ok_or_else(|| anyhow::anyhow!("no primary brightness display available"))
    }

    async fn write_display_value(&self, target: &ControlTarget, value: u32) -> anyhow::Result<()> {
        match target {
            ControlTarget::Internal(target) => self.internal.write_value(target, value).await,
            ControlTarget::Ddc(target) => self.external.write_value(target, value).await,
        }
    }
}

fn choose_brightness_session<'a>(
    current: Option<&'a Login1BrightnessSession>,
    sessions: &'a [Login1BrightnessSession],
    current_user: Option<&str>,
) -> Option<&'a Login1BrightnessSession> {
    current
        .filter(|session| session.supports_brightness())
        .or_else(|| {
            sessions.iter().find(|session| {
                session.supports_brightness()
                    && current_user.is_none_or(|user| session.user_name == user)
            })
        })
        .or_else(|| {
            sessions
                .iter()
                .find(|session| session.supports_brightness())
        })
}

fn filter_available_displays(displays: Vec<ControlDisplay>) -> Vec<ControlDisplay> {
    displays
        .into_iter()
        .filter(|display| display.data.available)
        .collect()
}

fn connected_connector_fingerprint(connectors: &[DrmConnectorState]) -> Vec<String> {
    let mut fingerprint = connectors
        .iter()
        .filter(|connector| connector.connected)
        .map(|connector| connector.connector.clone())
        .collect::<Vec<_>>();
    fingerprint.sort();
    fingerprint
}

fn is_retryable_ddcutil_error(stderr: &str) -> bool {
    stderr.contains("DDCRC_RETRIES") || stderr.contains("EREMOTEIO")
}

fn write_ddc_value(display_index: u32, value: u32) -> anyhow::Result<()> {
    let mut last_error = None;

    for attempt in 1..=DDC_WRITE_ATTEMPTS {
        let output = Command::new("ddcutil")
            .args(["setvcp", "10", &value.to_string(), "--display"])
            .arg(display_index.to_string())
            .output();

        match output {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
                if attempt < DDC_WRITE_ATTEMPTS && is_retryable_ddcutil_error(&stderr) {
                    tracing::debug!(
                        attempt,
                        display_index,
                        value,
                        stderr = %stderr,
                        "brightness provider: retrying transient ddcutil failure"
                    );
                    std::thread::sleep(DDC_WRITE_RETRY_DELAY);
                    last_error = Some(stderr);
                    continue;
                }

                anyhow::bail!("ddcutil setvcp failed: {stderr}");
            }
            Err(error) => {
                if attempt < DDC_WRITE_ATTEMPTS {
                    tracing::debug!(
                        attempt,
                        display_index,
                        value,
                        error = %error,
                        "brightness provider: retrying ddcutil command error"
                    );
                    std::thread::sleep(DDC_WRITE_RETRY_DELAY);
                    continue;
                }
                return Err(error.into());
            }
        }
    }

    anyhow::bail!(
        "ddcutil setvcp failed after {} attempts: {}",
        DDC_WRITE_ATTEMPTS,
        last_error.unwrap_or_else(|| "unknown error".into())
    );
}

fn lookup_cached_display(displays: &[ControlDisplay], display_id: &str) -> Option<ControlDisplay> {
    displays
        .iter()
        .find(|display| display.data.id == display_id)
        .cloned()
}

fn lookup_cached_primary_display(displays: &[ControlDisplay]) -> Option<ControlDisplay> {
    let visible = displays
        .iter()
        .map(|display| display.data.clone())
        .collect::<Vec<_>>();
    let primary_id = choose_primary_display(&visible)?.id.clone();
    lookup_cached_display(displays, &primary_id)
}

pub fn choose_primary_display(displays: &[BrightnessDisplay]) -> Option<&BrightnessDisplay> {
    displays
        .iter()
        .find(|display| display.is_primary && display.available)
        .or_else(|| {
            displays
                .iter()
                .find(|display| display.is_internal && display.available)
        })
        .or_else(|| displays.iter().find(|display| display.available))
}

fn classify_change(
    previous: &BrightnessSnapshot,
    next: &BrightnessSnapshot,
) -> BrightnessChangeReason {
    let displays_changed = previous
        .displays
        .iter()
        .map(|display| (&display.id, display.backend, display.is_internal))
        .ne(next
            .displays
            .iter()
            .map(|display| (&display.id, display.backend, display.is_internal)));
    let availability_changed = previous.status != next.status;
    let levels_changed = previous
        .displays
        .iter()
        .map(|display| (&display.id, display.current, display.percentage))
        .ne(next
            .displays
            .iter()
            .map(|display| (&display.id, display.current, display.percentage)));

    match (displays_changed, availability_changed, levels_changed) {
        (true, false, false) => BrightnessChangeReason::DisplaysChanged,
        (false, true, false) => BrightnessChangeReason::AvailabilityChanged,
        (false, false, true) => BrightnessChangeReason::LevelsChanged,
        _ => BrightnessChangeReason::Mixed,
    }
}

fn brightness_from_internal(
    internal: InternalBacklight,
    is_primary: bool,
    available: bool,
) -> BrightnessDisplay {
    BrightnessDisplay {
        id: internal.id,
        name: internal.name,
        backend: BrightnessBackend::Backlight,
        current: internal.current,
        max: internal.max,
        percentage: percentage(internal.current, internal.max),
        is_internal: true,
        is_primary,
        available,
    }
}

fn brightness_from_ddc(
    ddc: DdcDisplay,
    is_primary: bool,
    is_internal: bool,
    available: bool,
) -> BrightnessDisplay {
    BrightnessDisplay {
        id: ddc.id,
        name: ddc.name,
        backend: BrightnessBackend::Ddc,
        current: ddc.current,
        max: ddc.max,
        percentage: percentage(ddc.current, ddc.max),
        is_internal,
        is_primary,
        available,
    }
}

fn percentage(current: u32, max: u32) -> u8 {
    if max == 0 {
        return 0;
    }
    ((current.saturating_mul(100)) / max).min(100) as u8
}

fn raw_value_for_percent(percent: u8, max: u32, is_internal: bool) -> u32 {
    let raw = (u64::from(percent).saturating_mul(u64::from(max.max(1))) / 100) as u32;
    clamp_brightness(raw, max, is_internal)
}

fn clamp_brightness(value: u32, max: u32, is_internal: bool) -> u32 {
    let minimum = if is_internal {
        MIN_INTERNAL_BRIGHTNESS
    } else {
        0
    };
    value.clamp(minimum.min(max), max)
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

fn display_availability(
    connector: Option<&str>,
    is_internal: bool,
    connectors: &[DrmConnectorState],
) -> bool {
    if let Some(connector) = connector {
        return connectors
            .iter()
            .find(|state| state.connector == connector)
            .map(|state| state.connected && state.enabled)
            .unwrap_or(true);
    }

    if is_internal {
        if let Some(connector) = preferred_internal_connector(connectors) {
            return connector.connected && connector.enabled;
        }
    }

    true
}

fn preferred_internal_connector(connectors: &[DrmConnectorState]) -> Option<&DrmConnectorState> {
    connectors
        .iter()
        .filter(|connector| connector.kind == DisplayConnectorKind::Internal)
        .max_by_key(|connector| (connector.enabled, connector.connected))
}

fn discover_internal_backlight(connectors: &[DrmConnectorState]) -> Option<InternalBacklight> {
    let entries = fs::read_dir(SYS_BACKLIGHT_DIR).ok()?;
    let mut displays = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(display) = read_internal_backlight(&path, connectors) {
            displays.push(display);
        }
    }

    select_preferred_internal(&displays).cloned()
}

fn read_internal_backlight(
    path: &Path,
    connectors: &[DrmConnectorState],
) -> Option<InternalBacklight> {
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
        connector: preferred_internal_connector(connectors)
            .map(|connector| connector.connector.clone()),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DetectedDdcDisplay {
    index: u32,
    name: String,
    connector: Option<String>,
}

fn parse_ddcutil_detect(output: &str) -> Vec<DetectedDdcDisplay> {
    let mut displays = Vec::new();
    let mut current_index: Option<u32> = None;
    let mut current_name: Option<String> = None;
    let mut current_connector: Option<String> = None;

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("Display ") {
            if let Some(index) = current_index.take() {
                displays.push(DetectedDdcDisplay {
                    index,
                    name: current_name
                        .take()
                        .unwrap_or_else(|| format!("Display {index}")),
                    connector: current_connector.take(),
                });
            }
            current_index = rest.parse::<u32>().ok();
            current_name = None;
            current_connector = None;
            continue;
        }

        if let Some(name) = line.strip_prefix("Monitor:") {
            current_name = Some(name.trim().to_owned());
        } else if let Some(connector) = line.strip_prefix("DRM connector:") {
            let connector = normalize_connector_name(connector.trim()).to_owned();
            current_connector = Some(connector.clone());
            current_name.get_or_insert(connector);
        }
    }

    if let Some(index) = current_index {
        displays.push(DetectedDdcDisplay {
            index,
            name: current_name.unwrap_or_else(|| format!("Display {index}")),
            connector: current_connector,
        });
    }

    displays
}

fn ddc_display_from_detected(
    detected: DetectedDdcDisplay,
    value: DdcVcpValue,
    connectors: &[DrmConnectorState],
) -> Option<DdcDisplay> {
    let connector = detected
        .connector
        .filter(|connector| connectors.iter().any(|state| state.connector == *connector));

    Some(DdcDisplay {
        id: format!("ddc:{}", detected.index),
        name: detected.name,
        connector,
        index: detected.index,
        current: value.current,
        max: value.max,
    })
}

fn parse_ddcutil_getvcp_terse(output: &str) -> Option<DdcVcpValue> {
    let parts = output.split_whitespace().collect::<Vec<_>>();
    let (current, max) = match parts.as_slice() {
        [code, _kind, current, max] if *code == "10" => (*current, *max),
        ["VCP", code, _kind, current, max] if *code == "10" => (*current, *max),
        _ => return None,
    };

    Some(DdcVcpValue {
        current: current.parse().ok()?,
        max: max.parse().ok()?,
    })
}

fn read_u32(path: PathBuf) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{
        BacklightType, BrightnessBackend, BrightnessChangeReason, BrightnessDisplay,
        BrightnessSnapshot, ControlDisplay, ControlTarget, DdcDisplay, DdcVcpValue,
        DetectedDdcDisplay, ExternalControlTarget, InternalBacklight, Login1BrightnessSession,
        choose_brightness_session, choose_primary_display, classify_change,
        ddc_display_from_detected, display_availability, filter_available_displays,
        is_retryable_ddcutil_error, lookup_cached_display, parse_ddcutil_getvcp_terse,
        raw_value_for_percent, select_preferred_internal,
    };
    use crate::display::{DisplayConnectorKind, DrmConnectorState};
    use zbus::zvariant::OwnedObjectPath;

    #[test]
    fn select_preferred_internal_uses_firmware_priority() {
        let displays = vec![
            InternalBacklight {
                id: "raw".into(),
                name: "Raw".into(),
                connector: None,
                device_name: "raw".into(),
                current: 20,
                max: 100,
                backlight_type: BacklightType::Raw,
            },
            InternalBacklight {
                id: "platform".into(),
                name: "Platform".into(),
                connector: None,
                device_name: "platform".into(),
                current: 25,
                max: 100,
                backlight_type: BacklightType::Platform,
            },
            InternalBacklight {
                id: "firmware".into(),
                name: "Firmware".into(),
                connector: None,
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
                max: 100
            }
        );
    }

    #[test]
    fn parse_ddcutil_getvcp_terse_accepts_vcp_prefixed_output() {
        let parsed = parse_ddcutil_getvcp_terse("VCP 10 C 97 100").expect("parsed terse output");
        assert_eq!(
            parsed,
            DdcVcpValue {
                current: 97,
                max: 100
            }
        );
    }

    #[test]
    fn choose_primary_display_prefers_explicit_primary_before_internal() {
        let displays = vec![
            BrightnessDisplay {
                id: "backlight:intel".into(),
                name: "Laptop".into(),
                backend: BrightnessBackend::Backlight,
                current: 1200,
                max: 2000,
                percentage: 60,
                is_internal: true,
                is_primary: false,
                available: true,
            },
            BrightnessDisplay {
                id: "ddc:1".into(),
                name: "Dell".into(),
                backend: BrightnessBackend::Ddc,
                current: 40,
                max: 100,
                percentage: 40,
                is_internal: false,
                is_primary: true,
                available: true,
            },
        ];

        let primary = choose_primary_display(&displays).expect("primary display");
        assert_eq!(primary.id, "ddc:1");
    }

    #[test]
    fn raw_value_for_percent_clamps_internal_zero_to_minimum() {
        assert_eq!(raw_value_for_percent(0, 2000, true), 1);
        assert_eq!(raw_value_for_percent(0, 100, false), 0);
    }

    #[test]
    fn classify_change_distinguishes_level_updates() {
        let previous = BrightnessSnapshot::new(vec![BrightnessDisplay {
            id: "backlight:intel".into(),
            name: "Laptop".into(),
            backend: BrightnessBackend::Backlight,
            current: 1000,
            max: 2000,
            percentage: 50,
            is_internal: true,
            is_primary: true,
            available: true,
        }]);
        let next = BrightnessSnapshot::new(vec![BrightnessDisplay {
            current: 1200,
            percentage: 60,
            ..previous.displays[0].clone()
        }]);

        assert_eq!(
            classify_change(&previous, &next),
            BrightnessChangeReason::LevelsChanged
        );
    }

    #[test]
    fn choose_brightness_session_skips_manager_session() {
        let current = Login1BrightnessSession {
            path: object_path("/org/freedesktop/login1/session/_34"),
            user_name: "alex".into(),
            seat: String::new(),
            class: "manager".into(),
            kind: "unspecified".into(),
            active: true,
        };
        let user = Login1BrightnessSession {
            path: object_path("/org/freedesktop/login1/session/_33"),
            user_name: "alex".into(),
            seat: "seat0".into(),
            class: "user".into(),
            kind: "wayland".into(),
            active: true,
        };

        let chosen =
            choose_brightness_session(Some(&current), std::slice::from_ref(&user), Some("alex"))
                .expect("brightness session");

        assert_eq!(chosen.path, user.path);
    }

    #[test]
    fn display_availability_ignores_disabled_internal_panel() {
        let connectors = vec![
            DrmConnectorState {
                connector: "eDP-1".into(),
                kind: DisplayConnectorKind::Internal,
                connected: true,
                enabled: false,
            },
            DrmConnectorState {
                connector: "DP-2".into(),
                kind: DisplayConnectorKind::External,
                connected: true,
                enabled: true,
            },
        ];

        assert!(!display_availability(None, true, &connectors));
        assert!(display_availability(Some("DP-2"), false, &connectors));
    }

    #[test]
    fn ddc_display_from_detected_matches_known_connector() {
        let connectors = vec![DrmConnectorState {
            connector: "DP-2".into(),
            kind: DisplayConnectorKind::External,
            connected: true,
            enabled: true,
        }];
        let detected = DetectedDdcDisplay {
            index: 1,
            name: "Dell".into(),
            connector: Some("DP-2".into()),
        };

        let matched = ddc_display_from_detected(
            detected,
            DdcVcpValue {
                current: 57,
                max: 100,
            },
            &connectors,
        )
        .expect("matched DDC display");

        assert_eq!(
            matched,
            DdcDisplay {
                id: "ddc:1".into(),
                name: "Dell".into(),
                connector: Some("DP-2".into()),
                index: 1,
                current: 57,
                max: 100,
            }
        );
    }

    #[test]
    fn filter_available_displays_drops_disabled_targets() {
        let displays = vec![
            ControlDisplay {
                target: ControlTarget::Ddc(ExternalControlTarget { display_index: 1 }),
                data: BrightnessDisplay {
                    id: "ddc:1".into(),
                    name: "Docked monitor".into(),
                    backend: BrightnessBackend::Ddc,
                    current: 51,
                    max: 100,
                    percentage: 51,
                    is_internal: false,
                    is_primary: true,
                    available: true,
                },
            },
            ControlDisplay {
                target: ControlTarget::Ddc(ExternalControlTarget { display_index: 2 }),
                data: BrightnessDisplay {
                    id: "backlight:amdgpu_bl1".into(),
                    name: "Laptop panel".into(),
                    backend: BrightnessBackend::Backlight,
                    current: 50,
                    max: 100,
                    percentage: 50,
                    is_internal: true,
                    is_primary: false,
                    available: false,
                },
            },
        ];

        let filtered = filter_available_displays(displays);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].data.id, "ddc:1");
    }

    #[test]
    fn lookup_cached_display_uses_existing_discovery_state() {
        let cached = vec![ControlDisplay {
            target: ControlTarget::Ddc(ExternalControlTarget { display_index: 1 }),
            data: BrightnessDisplay {
                id: "ddc:1".into(),
                name: "Docked monitor".into(),
                backend: BrightnessBackend::Ddc,
                current: 51,
                max: 100,
                percentage: 51,
                is_internal: false,
                is_primary: true,
                available: true,
            },
        }];

        let display = lookup_cached_display(&cached, "ddc:1").expect("cached display");

        assert_eq!(
            display.target,
            ControlTarget::Ddc(ExternalControlTarget { display_index: 1 })
        );
        assert_eq!(display.data.name, "Docked monitor");
    }

    #[test]
    fn retryable_ddcutil_errors_are_detected() {
        assert!(is_retryable_ddcutil_error(
            "Setting value failed for feature x10, rc=DDCRC_RETRIES(-3007): maximum retries exceeded"
        ));
        assert!(is_retryable_ddcutil_error(
            "Error detecting VCP version using VCP feature xDF: Error_Info[DDCRC_RETRIES in ddc_write_read_with_retry, causes: EREMOTEIO(10)]"
        ));
        assert!(!is_retryable_ddcutil_error("Permission denied"));
    }

    fn object_path(path: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(path).expect("valid object path")
    }
}
