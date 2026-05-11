use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::{
    process::Command as TokioCommand,
    sync::{mpsc, watch},
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

use crate::{
    dbus::{
        login1::{Login1ManagerProxy, Login1SessionProxy},
        upower::UPowerKbdBacklightProxy,
    },
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

#[derive(Debug)]
struct UnsupportedBrightnessSource;

impl std::fmt::Display for UnsupportedBrightnessSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unsupported brightness source")
    }
}

impl std::error::Error for UnsupportedBrightnessSource {}

const COMMAND_QUEUE_SIZE: usize = 32;
const APPLY_DELAY: Duration = Duration::from_millis(100);
const RETRY_DELAY: Duration = Duration::from_secs(5);
const DDCUTIL_TIMEOUT: Duration = Duration::from_secs(4);
const BACKLIGHT_ROOT: &str = "/sys/class/backlight";
const LED_ROOT: &str = "/sys/class/leds";
const DRM_ROOT: &str = "/sys/class/drm";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum BrightnessSourceKind {
    BuiltInDisplay,
    ExternalDisplay,
    Keyboard,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrightnessSource {
    pub id: String,
    pub name: String,
    pub kind: BrightnessSourceKind,
    pub icon: String,
    pub current: u32,
    pub max: u32,
    pub percent: u8,
    pub writable: bool,
    pub primary: bool,
    pub available: bool,
}

impl BrightnessSource {
    pub fn is_usable(&self) -> bool {
        self.available && self.writable
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub available: bool,
    pub sources: Vec<BrightnessSource>,
    pub active: Option<ActiveAdjustment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveAdjustment {
    SetPercent { id: String, percent: u8 },
    AdjustPercent { id: String, delta: i32 },
}

impl ActiveAdjustment {
    fn id(&self) -> &str {
        match self {
            Self::SetPercent { id, .. } | Self::AdjustPercent { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Refresh,
    SetPercent { id: String, percent: u8 },
    AdjustPercent { id: String, delta: i32 },
}

pub type BrightnessHandle = ServiceHandle<State, Command>;

#[async_trait]
trait BrightnessBackend: Send + Sync {
    async fn scan(&self) -> Vec<BrightnessSource>;
    async fn set_percent(&self, id: &str, percent: u8) -> anyhow::Result<()>;
}

pub struct BrightnessService {
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    backends: Vec<Arc<dyn BrightnessBackend>>,
}

enum ApplyMessage {
    Ready(String),
    Complete {
        id: String,
        percent: u8,
        result: anyhow::Result<()>,
    },
}

enum RunOutcome {
    Cancelled,
    RetryAfterDelay,
}

#[derive(Default)]
struct ApplyQueue {
    pending: HashMap<String, u8>,
    scheduled: HashSet<String>,
    in_flight: HashMap<String, u8>,
}

impl ApplyQueue {
    fn targets(&self) -> HashMap<String, u8> {
        let mut targets = self.in_flight.clone();
        targets.extend(
            self.pending
                .iter()
                .map(|(id, percent)| (id.clone(), *percent)),
        );
        targets
    }
}

impl BrightnessService {
    pub fn new(system_dbus: zbus::Connection) -> (Self, BrightnessHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);

        let backends: Vec<Arc<dyn BrightnessBackend>> = vec![
            Arc::new(BacklightBackend::new(system_dbus.clone())),
            Arc::new(KeyboardBacklightBackend::new(system_dbus.clone())),
            Arc::new(LedBackend::new(
                PathBuf::from(LED_ROOT),
                system_dbus.clone(),
            )),
            Arc::new(DdcutilBackend::new()),
        ];

        (
            Self {
                state_tx,
                command_rx,
                backends,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    #[cfg(test)]
    fn with_backends(backends: Vec<Arc<dyn BrightnessBackend>>) -> (Self, BrightnessHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);
        (
            Self {
                state_tx,
                command_rx,
                backends,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        loop {
            let outcome = match self.run_inner(cancel.clone()).await {
                Ok(outcome) => outcome,
                Err(error) => {
                    tracing::warn!(%error, "brightness service failed");
                    self.change_state(State::default());
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
        let (apply_tx, mut apply_rx) = mpsc::channel(COMMAND_QUEUE_SIZE);
        let mut queue = ApplyQueue::default();

        self.refresh().await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(RunOutcome::Cancelled),
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Control(Control::Shutdown)) | None => {
                        return Ok(RunOutcome::Cancelled);
                    }
                    Some(ServiceCommand::Control(Control::Start(_) | Control::Reconfigure(_)))
                    | Some(ServiceCommand::Command(Command::Refresh)) => {
                        self.refresh_preserving_targets(&queue).await;
                    }
                    Some(ServiceCommand::Command(Command::SetPercent { id, percent })) => {
                        self.queue_set(&mut queue, apply_tx.clone(), id, percent);
                    }
                    Some(ServiceCommand::Command(Command::AdjustPercent { id, delta })) => {
                        self.queue_adjust(&mut queue, apply_tx.clone(), id, delta);
                    }
                },
                message = apply_rx.recv() => match message {
                    Some(ApplyMessage::Ready(id)) => {
                        self.start_apply(&mut queue, apply_tx.clone(), id);
                    }
                    Some(ApplyMessage::Complete { id, percent, result }) => {
                        queue.in_flight.remove(&id);
                        if let Err(error) = result {
                            tracing::warn!(%error, source = %id, percent, "brightness apply failed");
                            self.refresh().await;
                        }
                        if !queue.pending.contains_key(&id) {
                            self.clear_active(&id);
                        }
                        if queue.pending.contains_key(&id) {
                            schedule_apply(&mut queue, apply_tx.clone(), id);
                        }
                    }
                    None => return Ok(RunOutcome::Cancelled),
                }
            }
        }
    }

    async fn refresh(&self) {
        let mut sources = Vec::new();
        for backend in &self.backends {
            sources.extend(backend.scan().await);
        }
        normalize_sources(&mut sources);
        self.publish_refreshed_sources(sources, HashMap::new());
    }

    async fn refresh_preserving_targets(&self, queue: &ApplyQueue) {
        let mut sources = Vec::new();
        for backend in &self.backends {
            sources.extend(backend.scan().await);
        }
        normalize_sources(&mut sources);
        self.publish_refreshed_sources(sources, queue.targets());
    }

    fn publish_refreshed_sources(
        &self,
        mut sources: Vec<BrightnessSource>,
        targets: HashMap<String, u8>,
    ) {
        overlay_targets(&mut sources, &targets);
        let current_active = self.state_tx.borrow().active.clone();
        let active = current_active.filter(|active| targets.contains_key(active.id()));
        self.change_state(State {
            available: sources.iter().any(BrightnessSource::is_usable),
            sources,
            active,
        });
    }

    fn queue_set(
        &self,
        queue: &mut ApplyQueue,
        apply_tx: mpsc::Sender<ApplyMessage>,
        id: String,
        percent: u8,
    ) {
        let percent = percent.min(100);
        self.optimistic_update(ActiveAdjustment::SetPercent {
            id: id.clone(),
            percent,
        });
        queue.pending.insert(id.clone(), percent);
        schedule_apply(queue, apply_tx, id);
    }

    fn queue_adjust(
        &self,
        queue: &mut ApplyQueue,
        apply_tx: mpsc::Sender<ApplyMessage>,
        id: String,
        delta: i32,
    ) {
        let current = self
            .state_tx
            .borrow()
            .sources
            .iter()
            .find(|source| source.id == id)
            .map(|source| source.percent)
            .unwrap_or_default();
        let percent = adjust_percent(current, delta);
        self.optimistic_update(ActiveAdjustment::AdjustPercent {
            id: id.clone(),
            delta,
        });
        queue.pending.insert(id.clone(), percent);
        schedule_apply(queue, apply_tx, id);
    }

    fn optimistic_update(&self, active: ActiveAdjustment) {
        let mut next = self.state_tx.borrow().clone();
        let (id, percent) = match &active {
            ActiveAdjustment::SetPercent { id, percent } => (id.as_str(), *percent),
            ActiveAdjustment::AdjustPercent { id, delta } => {
                let percent = next
                    .sources
                    .iter()
                    .find(|source| source.id == *id)
                    .map(|source| adjust_percent(source.percent, *delta))
                    .unwrap_or_default();
                (id.as_str(), percent)
            }
        };
        if let Some(source) = next.sources.iter_mut().find(|source| source.id == id) {
            source.percent = percent;
            source.current = value_from_percent(source.max, percent);
        }
        next.active = Some(active);
        self.change_state(next);
    }

    fn start_apply(
        &self,
        queue: &mut ApplyQueue,
        apply_tx: mpsc::Sender<ApplyMessage>,
        id: String,
    ) {
        queue.scheduled.remove(&id);
        if queue.in_flight.contains_key(&id) {
            return;
        }
        let Some(percent) = queue.pending.remove(&id) else {
            return;
        };
        queue.in_flight.insert(id.clone(), percent);
        let backends = self.backends.clone();
        tokio::spawn(async move {
            let result = set_with_backends(backends, &id, percent).await;
            let _ = apply_tx
                .send(ApplyMessage::Complete {
                    id,
                    percent,
                    result,
                })
                .await;
        });
    }

    fn change_state(&self, state: State) {
        if *self.state_tx.borrow() == state {
            return;
        }
        if let Err(error) = self.state_tx.send(state) {
            tracing::error!(?error, "failed to publish brightness state");
        }
    }

    fn clear_active(&self, id: &str) {
        let mut next = self.state_tx.borrow().clone();
        let Some(active) = &next.active else {
            return;
        };
        let active_id = match active {
            ActiveAdjustment::SetPercent { id, .. }
            | ActiveAdjustment::AdjustPercent { id, .. } => id,
        };
        if active_id != id {
            return;
        }
        next.active = None;
        self.change_state(next);
    }
}

async fn set_with_backends(
    backends: Vec<Arc<dyn BrightnessBackend>>,
    id: &str,
    percent: u8,
) -> anyhow::Result<()> {
    for backend in backends {
        match backend.set_percent(id, percent).await {
            Ok(()) => return Ok(()),
            Err(error) if error.is::<UnsupportedBrightnessSource>() => {}
            Err(error) => return Err(error),
        }
    }
    anyhow::bail!("unsupported brightness source")
}

fn schedule_apply(queue: &mut ApplyQueue, apply_tx: mpsc::Sender<ApplyMessage>, id: String) {
    if queue.scheduled.contains(&id) || queue.in_flight.contains_key(&id) {
        return;
    }
    queue.scheduled.insert(id.clone());
    tokio::spawn(async move {
        sleep(APPLY_DELAY).await;
        let _ = apply_tx.send(ApplyMessage::Ready(id)).await;
    });
}

fn normalize_sources(sources: &mut Vec<BrightnessSource>) {
    remove_led_keyboard_fallback_if_upower_keyboard_exists(sources);

    sources.sort_by(|left, right| {
        (
            source_kind_rank(left.kind),
            !left.primary,
            left.name.as_str(),
            left.id.as_str(),
        )
            .cmp(&(
                source_kind_rank(right.kind),
                !right.primary,
                right.name.as_str(),
                right.id.as_str(),
            ))
    });

    let mut has_primary = sources
        .iter()
        .any(|source| source.primary && source.is_usable());
    if !has_primary {
        if let Some(source) = sources.iter_mut().find(|source| source.is_usable()) {
            source.primary = true;
            has_primary = true;
        }
    }

    if has_primary {
        let mut primary_seen = false;
        for source in sources {
            if source.primary && source.is_usable() && !primary_seen {
                primary_seen = true;
            } else {
                source.primary = false;
            }
        }
    }
}

fn remove_led_keyboard_fallback_if_upower_keyboard_exists(sources: &mut Vec<BrightnessSource>) {
    let has_upower_keyboard = sources
        .iter()
        .any(|source| source.id == "keyboard:upower" && source.is_usable());
    if !has_upower_keyboard {
        return;
    }

    sources.retain(|source| {
        !(source.kind == BrightnessSourceKind::Keyboard && source.id.starts_with("led:"))
    });
}

fn overlay_targets(sources: &mut [BrightnessSource], targets: &HashMap<String, u8>) {
    for source in sources {
        let Some(percent) = targets.get(&source.id).copied() else {
            continue;
        };
        source.percent = percent;
        source.current = value_from_percent(source.max, percent);
    }
}

fn source_kind_rank(kind: BrightnessSourceKind) -> u8 {
    match kind {
        BrightnessSourceKind::BuiltInDisplay => 0,
        BrightnessSourceKind::ExternalDisplay => 1,
        BrightnessSourceKind::Keyboard => 2,
        BrightnessSourceKind::Other => 3,
    }
}

fn percent_from_value(current: u32, max: u32) -> u8 {
    if max == 0 {
        return 0;
    }
    ((current as f64 / max as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

fn value_from_percent(max: u32, percent: u8) -> u32 {
    ((max as f64 * percent.min(100) as f64) / 100.0)
        .round()
        .clamp(0.0, max as f64) as u32
}

fn adjust_percent(current: u8, delta: i32) -> u8 {
    (current as i32 + delta).clamp(0, 100) as u8
}

fn safe_percent_for_kind(kind: BrightnessSourceKind, percent: u8) -> u8 {
    match kind {
        BrightnessSourceKind::BuiltInDisplay => percent.clamp(1, 100),
        BrightnessSourceKind::ExternalDisplay
        | BrightnessSourceKind::Keyboard
        | BrightnessSourceKind::Other => percent.min(100),
    }
}

fn source_icon(kind: BrightnessSourceKind) -> &'static str {
    match kind {
        BrightnessSourceKind::BuiltInDisplay => "display-brightness-symbolic",
        BrightnessSourceKind::ExternalDisplay => "video-display-symbolic",
        BrightnessSourceKind::Keyboard => "input-keyboard-symbolic",
        BrightnessSourceKind::Other => "preferences-system-symbolic",
    }
}

struct BacklightBackend {
    root: PathBuf,
    conn: zbus::Connection,
}

impl BacklightBackend {
    fn new(conn: zbus::Connection) -> Self {
        Self {
            root: PathBuf::from(BACKLIGHT_ROOT),
            conn,
        }
    }
}

#[async_trait]
impl BrightnessBackend for BacklightBackend {
    async fn scan(&self) -> Vec<BrightnessSource> {
        scan_backlight_sources(&self.root)
    }

    async fn set_percent(&self, id: &str, percent: u8) -> anyhow::Result<()> {
        let Some(device) = id.strip_prefix("backlight:") else {
            return Err(UnsupportedBrightnessSource.into());
        };
        let path = self.root.join(device);
        let max = read_u32(&path.join("max_brightness"))
            .ok_or_else(|| anyhow::anyhow!("missing max_brightness for {device}"))?;
        let value = value_from_percent(
            max,
            safe_percent_for_kind(BrightnessSourceKind::BuiltInDisplay, percent),
        );
        if let Err(error) = set_brightness_via_logind(&self.conn, "backlight", device, value).await
        {
            tracing::debug!(%error, device, "logind brightness write failed, using sysfs fallback");
            fs::write(path.join("brightness"), value.to_string())?;
        }
        Ok(())
    }
}

async fn set_brightness_via_logind(
    conn: &zbus::Connection,
    subsystem: &str,
    device: &str,
    value: u32,
) -> anyhow::Result<()> {
    let manager = Login1ManagerProxy::new(conn).await?;
    let session_path = manager.get_session_by_pid(std::process::id()).await?;
    let session = Login1SessionProxy::builder(conn)
        .path(session_path)?
        .build()
        .await?;
    session.set_brightness(subsystem, device, value).await?;
    Ok(())
}

fn scan_backlight_sources(root: &Path) -> Vec<BrightnessSource> {
    let mut sources = fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| backlight_source_from_path(&entry.path()))
        .collect::<Vec<_>>();

    sources.sort_by(|left, right| left.name.cmp(&right.name));
    if let Some(first) = sources.first_mut() {
        first.primary = true;
    }
    sources
}

fn backlight_source_from_path(path: &Path) -> Option<BrightnessSource> {
    let device = path.file_name()?.to_string_lossy().to_string();
    let current = read_u32(&path.join("brightness"))?;
    let max = read_u32(&path.join("max_brightness"))?;
    if max == 0 {
        return None;
    }

    Some(BrightnessSource {
        id: format!("backlight:{device}"),
        name: "Built-in display".into(),
        kind: BrightnessSourceKind::BuiltInDisplay,
        icon: source_icon(BrightnessSourceKind::BuiltInDisplay).into(),
        current,
        max,
        percent: percent_from_value(current, max),
        writable: path.join("brightness").exists(),
        primary: false,
        available: true,
    })
}

struct LedBackend {
    root: PathBuf,
    conn: zbus::Connection,
}

struct KeyboardBacklightBackend {
    conn: zbus::Connection,
}

impl KeyboardBacklightBackend {
    fn new(conn: zbus::Connection) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl BrightnessBackend for KeyboardBacklightBackend {
    async fn scan(&self) -> Vec<BrightnessSource> {
        let Ok(kbd) = UPowerKbdBacklightProxy::new(&self.conn).await else {
            return Vec::new();
        };
        let Ok(max) = kbd.get_max_brightness().await else {
            return Vec::new();
        };
        if max <= 0 {
            return Vec::new();
        }

        let current = kbd.get_brightness().await.unwrap_or(0).clamp(0, max);
        vec![BrightnessSource {
            id: "keyboard:upower".into(),
            name: "Keyboard backlight".into(),
            kind: BrightnessSourceKind::Keyboard,
            icon: source_icon(BrightnessSourceKind::Keyboard).into(),
            current: current as u32,
            max: max as u32,
            percent: percent_from_value(current as u32, max as u32),
            writable: true,
            primary: false,
            available: true,
        }]
    }

    async fn set_percent(&self, id: &str, percent: u8) -> anyhow::Result<()> {
        if id != "keyboard:upower" {
            return Err(UnsupportedBrightnessSource.into());
        }

        let kbd = UPowerKbdBacklightProxy::new(&self.conn).await?;
        let max = kbd.get_max_brightness().await?;
        if max <= 0 {
            anyhow::bail!("keyboard backlight is unavailable");
        }

        let value = value_from_percent(max as u32, percent) as i32;
        kbd.set_brightness(value).await?;
        Ok(())
    }
}

impl LedBackend {
    fn new(root: PathBuf, conn: zbus::Connection) -> Self {
        Self { root, conn }
    }
}

#[async_trait]
impl BrightnessBackend for LedBackend {
    async fn scan(&self) -> Vec<BrightnessSource> {
        scan_led_sources(&self.root)
    }

    async fn set_percent(&self, id: &str, percent: u8) -> anyhow::Result<()> {
        let Some(device) = id.strip_prefix("led:") else {
            return Err(UnsupportedBrightnessSource.into());
        };
        let path = self.root.join(device);
        let max = read_u32(&path.join("max_brightness"))
            .ok_or_else(|| anyhow::anyhow!("missing max_brightness for {device}"))?;
        let value = value_from_percent(max, percent);
        if let Err(error) = set_brightness_via_logind(&self.conn, "leds", device, value).await {
            tracing::debug!(%error, device, "logind LED brightness write failed, using sysfs fallback");
            fs::write(path.join("brightness"), value.to_string())?;
        }
        Ok(())
    }
}

fn scan_led_sources(root: &Path) -> Vec<BrightnessSource> {
    let mut sources = fs::read_dir(root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| led_source_from_path(&entry.path()))
        .collect::<Vec<_>>();
    sources.sort_by(|left, right| {
        (source_kind_rank(left.kind), left.name.as_str())
            .cmp(&(source_kind_rank(right.kind), right.name.as_str()))
    });
    sources
}

fn led_source_from_path(path: &Path) -> Option<BrightnessSource> {
    let device = path.file_name()?.to_string_lossy().to_string();
    let current = read_u32(&path.join("brightness"))?;
    let max = read_u32(&path.join("max_brightness"))?;
    if max <= 1 || ignored_led(&device) {
        return None;
    }
    let kind = if is_keyboard_led(&device) {
        BrightnessSourceKind::Keyboard
    } else {
        BrightnessSourceKind::Other
    };

    Some(BrightnessSource {
        id: format!("led:{device}"),
        name: display_name_from_device(&device),
        kind,
        icon: source_icon(kind).into(),
        current,
        max,
        percent: percent_from_value(current, max),
        writable: path.join("brightness").exists(),
        primary: false,
        available: true,
    })
}

fn ignored_led(device: &str) -> bool {
    let value = device.to_ascii_lowercase();
    value.contains("capslock")
        || value.contains("numlock")
        || value.contains("scrolllock")
        || value.contains("compose")
        || network_led(&value)
}

fn network_led(device: &str) -> bool {
    let compact = device.replace([':', '_', '-'], "");
    device.starts_with("phy")
        || device.contains("::phy")
        || compact.starts_with("enp")
        || compact.starts_with("ens")
        || compact.starts_with("eno")
        || compact.starts_with("eth")
        || compact.starts_with("wl")
        || device.contains("wlan")
        || device.contains("wifi")
        || device.contains("iwlwifi")
        || device.contains("ath9k")
        || device.contains("ath10k")
        || device.contains("mt76")
        || device.contains("r8169")
        || device.contains("realtek")
        || device.contains("ethernet")
        || device.contains("lan")
        || device.contains("link")
        || device.contains("rx")
        || device.contains("tx")
}

fn is_keyboard_led(device: &str) -> bool {
    let value = device.to_ascii_lowercase();
    value.contains("kbd") || value.contains("keyboard")
}

struct DdcutilBackend {
    drm_root: PathBuf,
}

impl DdcutilBackend {
    fn new() -> Self {
        Self {
            drm_root: PathBuf::from(DRM_ROOT),
        }
    }
}

#[async_trait]
impl BrightnessBackend for DdcutilBackend {
    async fn scan(&self) -> Vec<BrightnessSource> {
        let Ok(output) = command_output("ddcutil", &["detect", "--brief"]).await else {
            return Vec::new();
        };
        let mut sources = Vec::new();
        let mut seen_connectors = HashSet::new();
        for display in parse_ddcutil_detect(&output) {
            if !ddc_display_available(&self.drm_root, &display) {
                continue;
            }
            if let Some(connector) = display.connector.as_deref()
                && !seen_connectors.insert(connector.to_owned())
            {
                tracing::debug!(connector, "skipping duplicate ddc display");
                continue;
            }
            let index = display.index;
            let Ok(current) = ddcutil_current_percent(index).await else {
                tracing::debug!(display_index = index, "failed to read ddc brightness");
                continue;
            };
            sources.push(BrightnessSource {
                id: format!("ddcutil:{index}"),
                name: display.name,
                kind: BrightnessSourceKind::ExternalDisplay,
                icon: source_icon(BrightnessSourceKind::ExternalDisplay).into(),
                current,
                max: 100,
                percent: current as u8,
                writable: true,
                primary: false,
                available: true,
            });
        }
        sources
    }

    async fn set_percent(&self, id: &str, percent: u8) -> anyhow::Result<()> {
        let Some(index) = id.strip_prefix("ddcutil:") else {
            return Err(UnsupportedBrightnessSource.into());
        };
        command_status(
            "ddcutil",
            &[
                "setvcp",
                "10",
                &percent.min(100).to_string(),
                "--display",
                index,
            ],
        )
        .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DdcDisplay {
    index: u32,
    name: String,
    connector: Option<String>,
}

fn parse_ddcutil_detect(output: &str) -> Vec<DdcDisplay> {
    let mut displays = Vec::new();
    let mut current_index = None;
    let mut current_name = None;
    let mut current_connector = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Display ") {
            if let Some(index) = current_index.take() {
                displays.push(DdcDisplay {
                    index,
                    name: current_name
                        .take()
                        .unwrap_or_else(|| format!("Display {index}")),
                    connector: current_connector.take(),
                });
            }
            current_index = value
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u32>().ok());
            current_name = current_index.map(|index| format!("Display {index}"));
        } else if let Some(value) = trimmed.strip_prefix("DRM connector:") {
            let value = value.trim();
            if !value.is_empty() {
                current_name = Some(display_name_from_device(value));
                current_connector = drm_connector_from_device(value);
            }
        } else if let Some(value) = trimmed.strip_prefix("Monitor:") {
            let value = value.trim();
            if !value.is_empty() {
                current_name = Some(value.to_owned());
            }
        }
    }

    if let Some(index) = current_index {
        displays.push(DdcDisplay {
            index,
            name: current_name.unwrap_or_else(|| format!("Display {index}")),
            connector: current_connector,
        });
    }

    displays
}

fn ddc_display_available(drm_root: &Path, display: &DdcDisplay) -> bool {
    let Some(connector) = display.connector.as_deref() else {
        return true;
    };
    if is_internal_connector(connector) {
        return false;
    }
    drm_connector_connected(drm_root, connector).unwrap_or(true)
}

fn drm_connector_connected(drm_root: &Path, connector: &str) -> Option<bool> {
    let status = fs::read_to_string(drm_root.join(connector).join("status")).ok()?;
    Some(status.trim() == "connected")
}

fn drm_connector_from_device(device: &str) -> Option<String> {
    let _ = device.strip_prefix("card")?.split_once('-')?;
    Some(device.to_owned())
}

fn is_internal_connector(connector: &str) -> bool {
    let value = connector
        .strip_prefix("card")
        .and_then(|value| value.split_once('-').map(|(_, connector)| connector))
        .unwrap_or(connector)
        .to_ascii_lowercase();
    value.starts_with("edp") || value.starts_with("lvds") || value.starts_with("dsi")
}

async fn ddcutil_current_percent(index: u32) -> anyhow::Result<u32> {
    let output = command_output(
        "ddcutil",
        &["getvcp", "10", "--brief", "--display", &index.to_string()],
    )
    .await?;
    parse_ddcutil_getvcp_percent(&output)
        .ok_or_else(|| anyhow::anyhow!("failed to parse ddcutil brightness"))
}

fn parse_ddcutil_getvcp_percent(output: &str) -> Option<u32> {
    let numbers = output
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| {
            if part.is_empty() {
                None
            } else {
                part.parse::<u32>().ok()
            }
        })
        .collect::<Vec<_>>();
    if numbers.len() < 3 {
        return None;
    }
    let current = numbers[numbers.len() - 2];
    let max = numbers[numbers.len() - 1];
    Some(percent_from_value(current, max) as u32)
}

async fn command_output(program: &str, args: &[&str]) -> anyhow::Result<String> {
    let mut command = TokioCommand::new(program);
    command.args(args).kill_on_drop(true);
    let output = timeout(DDCUTIL_TIMEOUT, command.output())
        .await
        .map_err(|_| anyhow::anyhow!("{program} {} timed out", args.join(" ")))??;
    if !output.status.success() {
        anyhow::bail!("{program} {} failed", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn command_status(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let mut command = TokioCommand::new(program);
    command.args(args).kill_on_drop(true);
    let status = timeout(DDCUTIL_TIMEOUT, command.status())
        .await
        .map_err(|_| anyhow::anyhow!("{program} {} timed out", args.join(" ")))??;
    if !status.success() {
        anyhow::bail!("{program} {} failed", args.join(" "));
    }
    Ok(())
}

fn read_u32(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn display_name_from_device(device: &str) -> String {
    let trimmed = device
        .strip_prefix("card")
        .and_then(|value| value.split_once('-').map(|(_, connector)| connector))
        .unwrap_or(device);
    trimmed
        .replace(['_', ':'], " ")
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use super::*;

    #[test]
    fn percent_helpers_round_and_clamp_values() {
        assert_eq!(percent_from_value(50, 100), 50);
        assert_eq!(percent_from_value(1, 3), 33);
        assert_eq!(percent_from_value(9, 0), 0);
        assert_eq!(value_from_percent(255, 50), 128);
        assert_eq!(adjust_percent(5, -10), 0);
        assert_eq!(adjust_percent(95, 10), 100);
        assert_eq!(
            safe_percent_for_kind(BrightnessSourceKind::BuiltInDisplay, 0),
            1
        );
        assert_eq!(safe_percent_for_kind(BrightnessSourceKind::Keyboard, 0), 0);
    }

    #[test]
    fn scan_backlight_sources_marks_first_internal_display_primary() {
        let root = temp_dir("backlight");
        write_source(&root.join("intel_backlight"), 120, 240);

        let sources = scan_backlight_sources(&root);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "backlight:intel_backlight");
        assert_eq!(sources[0].name, "Built-in display");
        assert_eq!(sources[0].kind, BrightnessSourceKind::BuiltInDisplay);
        assert_eq!(sources[0].percent, 50);
        assert!(sources[0].primary);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn scan_led_sources_keeps_keyboard_and_other_brightness_sources() {
        let root = temp_dir("leds");
        write_source(&root.join("platform::kbd_backlight"), 1, 3);
        write_source(&root.join("tpacpi::thinklight"), 2, 4);
        write_source(&root.join("input3::capslock"), 1, 1);
        write_source(&root.join("phy0-led"), 128, 255);
        write_source(&root.join("iwlwifi_1::rx"), 128, 255);
        write_source(&root.join("r8169-0-300::link"), 128, 255);
        write_source(&root.join("enp4s0::link"), 128, 255);
        write_source(&root.join("enp4s0::rx"), 128, 255);
        write_source(&root.join("enp4s0::tx"), 128, 255);

        let sources = scan_led_sources(&root);

        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].kind, BrightnessSourceKind::Keyboard);
        assert_eq!(sources[1].kind, BrightnessSourceKind::Other);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parse_ddcutil_detect_uses_connector_name_when_available() {
        let displays = parse_ddcutil_detect(
            r#"
Display 1
   I2C bus: /dev/i2c-6
   DRM connector: card1-DP-3
Display 2
   I2C bus: /dev/i2c-7
   Monitor: Dell U2720Q
"#,
        );

        assert_eq!(
            displays,
            vec![
                DdcDisplay {
                    index: 1,
                    name: "DP 3".into(),
                    connector: Some("card1-DP-3".into()),
                },
                DdcDisplay {
                    index: 2,
                    name: "Dell U2720Q".into(),
                    connector: None,
                },
            ]
        );
    }

    #[test]
    fn ddc_display_available_skips_disconnected_and_internal_connectors() {
        let root = temp_dir("drm");
        write_drm_status(&root, "card1-DP-3", "disconnected");
        write_drm_status(&root, "card1-HDMI-A-1", "connected");

        assert!(!ddc_display_available(
            &root,
            &ddc_display("external", Some("card1-DP-3"))
        ));
        assert!(ddc_display_available(
            &root,
            &ddc_display("external", Some("card1-HDMI-A-1"))
        ));
        assert!(!ddc_display_available(
            &root,
            &ddc_display("internal", Some("card0-eDP-1"))
        ));
        assert!(ddc_display_available(&root, &ddc_display("unknown", None)));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn normalize_sources_ignores_unusable_primary_source() {
        let mut disabled = source(
            "ddcutil:disabled",
            BrightnessSourceKind::ExternalDisplay,
            true,
        );
        disabled.available = false;
        let mut sources = vec![
            disabled,
            source("led:kbd", BrightnessSourceKind::Keyboard, false),
        ];

        normalize_sources(&mut sources);

        assert_eq!(
            sources
                .iter()
                .find(|source| source.primary)
                .map(|source| source.id.as_str()),
            Some("led:kbd")
        );
    }

    #[test]
    fn normalize_sources_prefers_upower_keyboard_over_led_fallback() {
        let sources = &mut vec![
            source(
                "led:platform::kbd_backlight",
                BrightnessSourceKind::Keyboard,
                false,
            ),
            source("keyboard:upower", BrightnessSourceKind::Keyboard, false),
        ];

        normalize_sources(sources);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].id, "keyboard:upower");
    }

    #[test]
    fn parse_ddcutil_getvcp_percent_reads_current_and_max_values() {
        assert_eq!(parse_ddcutil_getvcp_percent("VCP 10 C 50 100"), Some(50));
        assert_eq!(
            parse_ddcutil_getvcp_percent(
                "VCP code 0x10 (Brightness): current value = 30, max value = 60"
            ),
            Some(50)
        );
    }

    #[test]
    fn normalize_sources_picks_single_writable_primary() {
        let mut sources = vec![
            source("led:kbd", BrightnessSourceKind::Keyboard, false),
            source("ddcutil:1", BrightnessSourceKind::ExternalDisplay, false),
        ];

        normalize_sources(&mut sources);

        assert_eq!(sources[0].id, "ddcutil:1");
        assert!(sources[0].primary);
        assert!(!sources[1].primary);
    }

    #[test]
    fn overlay_targets_keeps_pending_value_over_refreshed_hardware_value() {
        let mut sources = vec![source(
            "ddcutil:1",
            BrightnessSourceKind::ExternalDisplay,
            true,
        )];
        sources[0].current = 20;
        sources[0].percent = 20;

        overlay_targets(&mut sources, &HashMap::from([("ddcutil:1".into(), 80)]));

        assert_eq!(sources[0].percent, 80);
        assert_eq!(sources[0].current, 80);
    }

    #[tokio::test]
    async fn service_coalesces_slow_writes_to_latest_requested_percent() {
        let backend = Arc::new(TestBackend::default());
        let (service, handle) = BrightnessService::with_backends(vec![backend.clone()]);
        let cancel = CancellationToken::new();
        let task = tokio::spawn(service.run(cancel.clone()));

        handle
            .send(ServiceCommand::Command(Command::SetPercent {
                id: "test:primary".into(),
                percent: 10,
            }))
            .await
            .unwrap();
        handle
            .send(ServiceCommand::Command(Command::SetPercent {
                id: "test:primary".into(),
                percent: 80,
            }))
            .await
            .unwrap();

        tokio::time::sleep(APPLY_DELAY + Duration::from_millis(220)).await;
        let state = handle.snapshot();
        cancel.cancel();
        let _ = task.await;

        assert_eq!(backend.writes.load(Ordering::SeqCst), 1);
        assert_eq!(backend.last_percent.load(Ordering::SeqCst), 80);
        assert_eq!(state.active, None);
    }

    #[derive(Default)]
    struct TestBackend {
        writes: AtomicUsize,
        last_percent: AtomicUsize,
    }

    #[async_trait]
    impl BrightnessBackend for TestBackend {
        async fn scan(&self) -> Vec<BrightnessSource> {
            vec![source(
                "test:primary",
                BrightnessSourceKind::BuiltInDisplay,
                true,
            )]
        }

        async fn set_percent(&self, id: &str, percent: u8) -> anyhow::Result<()> {
            if id != "test:primary" {
                anyhow::bail!("unsupported brightness source");
            }
            self.writes.fetch_add(1, Ordering::SeqCst);
            self.last_percent.store(percent as usize, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(180)).await;
            Ok(())
        }
    }

    fn source(id: &str, kind: BrightnessSourceKind, primary: bool) -> BrightnessSource {
        BrightnessSource {
            id: id.into(),
            name: id.into(),
            kind,
            icon: source_icon(kind).into(),
            current: 50,
            max: 100,
            percent: 50,
            writable: true,
            primary,
            available: true,
        }
    }

    fn write_source(path: &Path, brightness: u32, max: u32) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("brightness"), brightness.to_string()).unwrap();
        fs::write(path.join("max_brightness"), max.to_string()).unwrap();
    }

    fn write_drm_status(root: &Path, connector: &str, status: &str) {
        let path = root.join(connector);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("status"), status).unwrap();
    }

    fn ddc_display(name: &str, connector: Option<&str>) -> DdcDisplay {
        DdcDisplay {
            index: 1,
            name: name.into(),
            connector: connector.map(Into::into),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "glimpse-brightness-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
}
