use std::{fmt, sync::{Arc, Mutex}, time::Duration};

use anyhow::{Context, bail};
use futures_util::{StreamExt, future};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use zbus::{MatchRule, MessageStream, message::Type, zvariant::ObjectPath};

use crate::dbus::bluez::{Adapter1Proxy, Battery1Proxy, Device1Proxy};

const LISTENER_DEBOUNCE: Duration = Duration::from_millis(300);
const INITIAL_DISCOVERY_WINDOW: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct BluetoothStatus {
    pub powered: bool,
    pub discovering: bool,
    pub connected_count: u32,
}

impl BluetoothStatus {
    fn from_parts(adapters: &[BluetoothAdapter], devices: &[BluetoothDevice]) -> Self {
        Self {
            powered: adapters.iter().any(|adapter| adapter.powered),
            discovering: adapters.iter().any(|adapter| adapter.discovering),
            connected_count: devices.iter().filter(|device| device.connected).count() as u32,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BluetoothAdapter {
    pub path: String,
    pub name: String,
    pub address: String,
    pub powered: bool,
    pub discovering: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum BluetoothDeviceType {
    Computer,
    Desktop,
    Laptop,
    Server,
    Tablet,
    Handheld,
    Pda,
    WearableComputer,
    Phone,
    Smartphone,
    Headphones,
    Earbud,
    Headset,
    HandsFree,
    Speaker,
    Soundbar,
    HiFiAudio,
    PortableAudio,
    CarAudio,
    Microphone,
    AudioSource,
    AudioVideo,
    DisplaySpeaker,
    Keyboard,
    Mouse,
    Touchpad,
    Joystick,
    Gamepad,
    Remote,
    PresentationRemote,
    Controller,
    DigitalPen,
    BarcodeScanner,
    CardReader,
    SensingDevice,
    Peripheral,
    Display,
    VideoMonitor,
    VideoConferencing,
    VideoCamera,
    Camcorder,
    SetTopBox,
    Vcr,
    MediaPlayer,
    Watch,
    Glasses,
    Wearable,
    Jacket,
    Helmet,
    HearingAid,
    Health,
    HeartRateSensor,
    BloodPressure,
    Thermometer,
    GlucoseMeter,
    FitnessTracker,
    CyclingSensor,
    Network,
    Imaging,
    Toy,
    Pager,
    Clock,
    Tag,
    Keyring,
    Sensor,
    Light,
    Gaming,
    #[default]
    Unknown,
}

impl BluetoothDeviceType {
    pub fn from_hints(appearance: u16, class: u32, icon_hint: &str) -> Self {
        if class != 0 {
            if let Some(kind) = Self::from_class(class) {
                return kind;
            }
        }

        if appearance != 0 {
            if let Some(kind) = Self::from_appearance(appearance) {
                return kind;
            }
        }

        match icon_hint {
            "audio-headphones" => Self::Headphones,
            "audio-headset" => Self::Headset,
            "audio-speakers" | "audio-card" => Self::Speaker,
            "input-keyboard" => Self::Keyboard,
            "input-mouse" => Self::Mouse,
            "input-tablet" => Self::Tablet,
            "input-gaming" => Self::Controller,
            "phone" => Self::Phone,
            "computer" => Self::Computer,
            "video-display" => Self::Display,
            _ => Self::Unknown,
        }
    }

    fn from_class(class: u32) -> Option<Self> {
        let major = (class >> 8) & 0x1F;
        let minor = (class >> 2) & 0x3F;

        match major {
            1 => Some(match minor {
                1 => Self::Desktop,
                2 => Self::Server,
                3 => Self::Laptop,
                4 => Self::Handheld,
                5 => Self::Pda,
                6 => Self::WearableComputer,
                7 => Self::Tablet,
                _ => Self::Computer,
            }),
            2 => Some(match minor {
                3 => Self::Smartphone,
                _ => Self::Phone,
            }),
            3 => Some(Self::Network),
            4 => Some(match minor {
                1 => Self::Headset,
                2 => Self::HandsFree,
                4 => Self::Microphone,
                5 => Self::Speaker,
                6 => Self::Headphones,
                7 => Self::PortableAudio,
                8 => Self::CarAudio,
                9 => Self::SetTopBox,
                10 => Self::HiFiAudio,
                11 => Self::Vcr,
                12 => Self::VideoCamera,
                13 => Self::Camcorder,
                14 => Self::VideoMonitor,
                15 => Self::DisplaySpeaker,
                16 => Self::VideoConferencing,
                18 => Self::Gaming,
                _ => Self::AudioVideo,
            }),
            5 => {
                let peripheral_type = (minor >> 4) & 0x03;
                let peripheral_subtype = minor & 0x0F;
                Some(match peripheral_type {
                    1 | 3 => Self::Keyboard,
                    2 => Self::Mouse,
                    _ => match peripheral_subtype {
                        1 => Self::Joystick,
                        2 => Self::Gamepad,
                        3 => Self::Remote,
                        4 => Self::SensingDevice,
                        5 => Self::Tablet,
                        6 => Self::CardReader,
                        7 => Self::DigitalPen,
                        8 => Self::BarcodeScanner,
                        _ => Self::Peripheral,
                    },
                })
            }
            6 => Some(Self::Imaging),
            7 => Some(match minor {
                1 => Self::Watch,
                2 => Self::Pager,
                3 => Self::Jacket,
                4 => Self::Helmet,
                5 => Self::Glasses,
                _ => Self::Wearable,
            }),
            8 => Some(Self::Toy),
            9 => Some(Self::Health),
            _ => None,
        }
    }

    fn from_appearance(appearance: u16) -> Option<Self> {
        let category = appearance >> 6;
        let subcategory = appearance & 0x3F;

        match category {
            1 => Some(Self::Phone),
            2 => Some(Self::Computer),
            3 => Some(Self::Watch),
            4 => Some(Self::Clock),
            5 => Some(Self::Display),
            6 => Some(Self::Remote),
            7 => Some(Self::Glasses),
            8 => Some(Self::Tag),
            9 => Some(Self::Keyring),
            10 => Some(Self::MediaPlayer),
            11 => Some(Self::BarcodeScanner),
            12 => Some(Self::Thermometer),
            13 => Some(Self::HeartRateSensor),
            14 => Some(Self::BloodPressure),
            15 => Some(match subcategory {
                1 => Self::Keyboard,
                2 => Self::Mouse,
                3 => Self::Joystick,
                4 => Self::Gamepad,
                5 => Self::Tablet,
                6 => Self::CardReader,
                7 => Self::DigitalPen,
                8 => Self::BarcodeScanner,
                9 => Self::Touchpad,
                10 => Self::PresentationRemote,
                _ => Self::Peripheral,
            }),
            16 => Some(Self::GlucoseMeter),
            17 => Some(Self::FitnessTracker),
            18 => Some(Self::CyclingSensor),
            21 => Some(Self::Sensor),
            22 => Some(Self::Light),
            33 => Some(match subcategory {
                2 => Self::Soundbar,
                _ => Self::Speaker,
            }),
            34 => Some(Self::AudioSource),
            37 => Some(match subcategory {
                1 => Self::Earbud,
                2 => Self::Headset,
                _ => Self::Headphones,
            }),
            41 => Some(Self::HearingAid),
            42 => Some(Self::Gaming),
            _ => None,
        }
    }

    pub fn icon(&self, connected: bool) -> &'static str {
        match self {
            Self::Headphones | Self::Earbud | Self::HearingAid => "audio-headphones-symbolic",
            Self::Headset | Self::HandsFree => "audio-headset-symbolic",
            Self::Speaker
            | Self::Soundbar
            | Self::HiFiAudio
            | Self::PortableAudio
            | Self::CarAudio
            | Self::AudioVideo
            | Self::DisplaySpeaker
            | Self::AudioSource => "audio-speakers-symbolic",
            Self::Microphone => "audio-input-microphone-symbolic",
            Self::Keyboard => "input-keyboard-symbolic",
            Self::Mouse | Self::Touchpad => "input-mouse-symbolic",
            Self::Tablet => "input-tablet-symbolic",
            Self::Joystick | Self::Gamepad | Self::Gaming | Self::Controller => {
                "input-gaming-symbolic"
            }
            Self::Remote | Self::PresentationRemote | Self::MediaPlayer => {
                "multimedia-player-symbolic"
            }
            Self::Phone | Self::Smartphone => "phone-symbolic",
            Self::Computer
            | Self::Desktop
            | Self::Server
            | Self::Laptop
            | Self::Handheld
            | Self::Pda
            | Self::WearableComputer => "computer-symbolic",
            Self::Display | Self::VideoMonitor | Self::VideoConferencing => {
                "video-display-symbolic"
            }
            Self::Watch | Self::Wearable | Self::Glasses | Self::Jacket | Self::Helmet => {
                "watch-symbolic"
            }
            Self::VideoCamera | Self::Camcorder => "camera-video-symbolic",
            Self::Imaging => "printer-symbolic",
            Self::Tag | Self::Keyring => "tag-symbolic",
            Self::Health
            | Self::HeartRateSensor
            | Self::BloodPressure
            | Self::Thermometer
            | Self::GlucoseMeter
            | Self::FitnessTracker => "heart-symbolic",
            Self::Unknown => {
                if connected {
                    "bluetooth-active-symbolic"
                } else {
                    "bluetooth-symbolic"
                }
            }
            _ => {
                tracing::warn!(device_type = ?self, "bluetooth: no icon mapping for device type, using generic");
                if connected {
                    "bluetooth-active-symbolic"
                } else {
                    "bluetooth-symbolic"
                }
            }
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Computer => "Computer",
            Self::Desktop => "Desktop",
            Self::Laptop => "Laptop",
            Self::Server => "Server",
            Self::Tablet => "Tablet",
            Self::Handheld => "Handheld",
            Self::Pda => "PDA",
            Self::WearableComputer => "Wearable Computer",
            Self::Phone => "Phone",
            Self::Smartphone => "Smartphone",
            Self::Headphones => "Headphones",
            Self::Earbud => "Earbud",
            Self::Headset => "Headset",
            Self::HandsFree => "Hands-free",
            Self::Speaker => "Speaker",
            Self::Soundbar => "Soundbar",
            Self::HiFiAudio => "HiFi Audio",
            Self::PortableAudio => "Portable Audio",
            Self::CarAudio => "Car Audio",
            Self::Microphone => "Microphone",
            Self::AudioSource => "Audio Source",
            Self::AudioVideo => "Audio/Video",
            Self::DisplaySpeaker => "Display Speaker",
            Self::Keyboard => "Keyboard",
            Self::Mouse => "Mouse",
            Self::Touchpad => "Touchpad",
            Self::Joystick => "Joystick",
            Self::Gamepad => "Gamepad",
            Self::Remote => "Remote",
            Self::PresentationRemote => "Presentation Remote",
            Self::Controller => "Controller",
            Self::DigitalPen => "Digital Pen",
            Self::BarcodeScanner => "Barcode Scanner",
            Self::CardReader => "Card Reader",
            Self::SensingDevice => "Sensing Device",
            Self::Peripheral => "Peripheral",
            Self::Display => "Display",
            Self::VideoMonitor => "Video Monitor",
            Self::VideoConferencing => "Video Conferencing",
            Self::VideoCamera => "Video Camera",
            Self::Camcorder => "Camcorder",
            Self::SetTopBox => "Set-top Box",
            Self::Vcr => "VCR",
            Self::MediaPlayer => "Media Player",
            Self::Watch => "Watch",
            Self::Glasses => "Glasses",
            Self::Wearable => "Wearable",
            Self::Jacket => "Jacket",
            Self::Helmet => "Helmet",
            Self::HearingAid => "Hearing Aid",
            Self::Health => "Health",
            Self::HeartRateSensor => "Heart Rate Sensor",
            Self::BloodPressure => "Blood Pressure",
            Self::Thermometer => "Thermometer",
            Self::GlucoseMeter => "Glucose Meter",
            Self::FitnessTracker => "Fitness Tracker",
            Self::CyclingSensor => "Cycling Sensor",
            Self::Network => "Network",
            Self::Imaging => "Imaging",
            Self::Toy => "Toy",
            Self::Pager => "Pager",
            Self::Clock => "Clock",
            Self::Tag => "Tag",
            Self::Keyring => "Keyring",
            Self::Sensor => "Sensor",
            Self::Light => "Light",
            Self::Gaming => "Gaming",
            Self::Unknown => "",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub address: String,
    pub name: String,
    pub device_type: BluetoothDeviceType,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    pub battery: Option<u8>,
    pub rssi: Option<i16>,
    pub adapter: String,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct BluetoothSnapshot {
    pub status: BluetoothStatus,
    pub adapters: Vec<BluetoothAdapter>,
    pub devices: Vec<BluetoothDevice>,
}

impl BluetoothSnapshot {
    fn new(mut adapters: Vec<BluetoothAdapter>, mut devices: Vec<BluetoothDevice>) -> Self {
        adapters.sort_by(|left, right| left.path.cmp(&right.path));
        devices.sort_by(|left, right| left.address.cmp(&right.address));

        let status = BluetoothStatus::from_parts(&adapters, &devices);

        Self {
            status,
            adapters,
            devices,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothChangeReason {
    InterfacesAdded,
    InterfacesRemoved,
    PropertiesChanged,
    Mixed,
}

impl fmt::Display for BluetoothChangeReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InterfacesAdded => "interfaces-added",
            Self::InterfacesRemoved => "interfaces-removed",
            Self::PropertiesChanged => "properties-changed",
            Self::Mixed => "mixed",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothProviderEvent {
    Changed { reason: BluetoothChangeReason },
}

#[derive(Clone)]
pub struct BluetoothProvider {
    conn: zbus::Connection,
    discovery: Arc<Mutex<DiscoveryClaims>>,
}

impl BluetoothProvider {
    pub fn new(conn: zbus::Connection) -> Self {
        Self {
            conn,
            discovery: Arc::new(Mutex::new(DiscoveryClaims::default())),
        }
    }

    pub async fn scan(&self) -> anyhow::Result<BluetoothSnapshot> {
        let om = self.object_manager().await?;
        let objects = om
            .get_managed_objects()
            .await
            .context("failed to read BlueZ managed objects")?;

        let mut adapters = Vec::new();
        let mut devices = Vec::new();

        for (path, interfaces) in &objects {
            let path_str = path.to_string();

            if interfaces.contains_key("org.bluez.Adapter1") {
                let adapter = self
                    .read_adapter(&path_str)
                    .await
                    .with_context(|| format!("failed to read adapter {path_str}"))?;
                adapters.push(adapter);
            }

            if interfaces.contains_key("org.bluez.Device1") {
                let device = self
                    .read_device(&path_str, interfaces.contains_key("org.bluez.Battery1"))
                    .await
                    .with_context(|| format!("failed to read device {path_str}"))?;

                if device.address.is_empty() {
                    tracing::debug!(path = %path_str, "bluetooth: skipping transient device without address");
                    continue;
                }

                devices.push(device);
            }
        }

        let snapshot = BluetoothSnapshot::new(adapters, devices);
        tracing::debug!(
            adapters = snapshot.adapters.len(),
            devices = snapshot.devices.len(),
            powered = snapshot.status.powered,
            discovering = snapshot.status.discovering,
            connected = snapshot.status.connected_count,
            "bluetooth: scan complete"
        );

        if snapshot.adapters.is_empty() {
            tracing::info!("bluetooth: no adapters found");
        } else if !snapshot.status.powered {
            tracing::info!(
                adapters = snapshot.adapters.len(),
                "bluetooth: adapters present but all are powered off"
            );
        }

        Ok(snapshot)
    }

    pub async fn listen(
        &self,
        events: mpsc::Sender<BluetoothProviderEvent>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        tracing::info!("bluetooth: listener started");

        let om = self.object_manager().await?;
        let mut added = om.receive_interfaces_added().await?;
        let mut removed = om.receive_interfaces_removed().await?;
        let mut properties = self.properties_changed_stream().await?;

        let mut initial_discovery_deadline = self.begin_initial_discovery().await;
        let mut pending_reason: Option<BluetoothChangeReason> = None;
        let mut debounce_deadline: Option<tokio::time::Instant> = None;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("bluetooth: listener stopping");
                    break;
                }
                _ = async {
                    match initial_discovery_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => future::pending::<()>().await,
                    }
                }, if initial_discovery_deadline.is_some() => {
                    initial_discovery_deadline = None;
                    match self.finish_initial_discovery().await {
                        Ok(()) => tracing::info!("bluetooth: initial discovery window finished"),
                        Err(error) => tracing::warn!(error = %error, "bluetooth: failed to stop initial discovery"),
                    }
                }
                signal = added.next() => {
                    match signal {
                        Some(_) => {
                            tracing::debug!("bluetooth: interfaces added signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, BluetoothChangeReason::InterfacesAdded));
                            debounce_deadline = Some(tokio::time::Instant::now() + LISTENER_DEBOUNCE);
                        }
                        None => {
                            tracing::warn!("bluetooth: interfaces-added stream ended");
                            break;
                        }
                    }
                }
                signal = removed.next() => {
                    match signal {
                        Some(_) => {
                            tracing::debug!("bluetooth: interfaces removed signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, BluetoothChangeReason::InterfacesRemoved));
                            debounce_deadline = Some(tokio::time::Instant::now() + LISTENER_DEBOUNCE);
                        }
                        None => {
                            tracing::warn!("bluetooth: interfaces-removed stream ended");
                            break;
                        }
                    }
                }
                signal = properties.next() => {
                    match signal {
                        Some(Ok(message)) => {
                            if !is_bluez_properties_changed(&message) {
                                continue;
                            }
                            tracing::debug!("bluetooth: properties changed signal received");
                            pending_reason = Some(merge_change_reason(pending_reason, BluetoothChangeReason::PropertiesChanged));
                            debounce_deadline = Some(tokio::time::Instant::now() + LISTENER_DEBOUNCE);
                        }
                        Some(Err(error)) => {
                            tracing::warn!(error = %error, "bluetooth: properties stream error");
                        }
                        None => {
                            tracing::warn!("bluetooth: properties stream ended");
                            break;
                        }
                    }
                }
                _ = async {
                    match debounce_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => future::pending::<()>().await,
                    }
                }, if debounce_deadline.is_some() => {
                    let reason = pending_reason.take().unwrap_or(BluetoothChangeReason::Mixed);
                    debounce_deadline = None;
                    tracing::debug!(reason = %reason, "bluetooth: change event emitted");
                    if events.send(BluetoothProviderEvent::Changed { reason }).await.is_err() {
                        tracing::info!("bluetooth: listener receiver dropped");
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn set_powered(&self, powered: bool) -> anyhow::Result<()> {
        let adapter_paths = self.adapter_paths().await?;
        if adapter_paths.is_empty() {
            bail!("no bluetooth adapters found");
        }

        tracing::info!(
            powered,
            adapters = adapter_paths.len(),
            "bluetooth: set power requested"
        );

        for path in adapter_paths {
            let proxy = self.adapter_proxy(&path).await?;
            proxy
                .set_powered(powered)
                .await
                .with_context(|| format!("failed to set adapter power on {path}"))?;
            tracing::debug!(path = %path, powered, "bluetooth: adapter power updated");
        }

        tracing::info!(powered, "bluetooth: set power succeeded");
        Ok(())
    }

    pub async fn connect(&self, address: &str) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            "bluetooth: connect requested"
        );
        let proxy = self.device_proxy(&device.path).await?;
        proxy
            .connect()
            .await
            .with_context(|| format!("failed to connect {}", device.address))?;
        tracing::info!(address = %device.address, name = %device.name, "bluetooth: connect succeeded");
        Ok(())
    }

    pub async fn disconnect(&self, address: &str) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            "bluetooth: disconnect requested"
        );
        let proxy = self.device_proxy(&device.path).await?;
        proxy
            .disconnect()
            .await
            .with_context(|| format!("failed to disconnect {}", device.address))?;
        tracing::info!(address = %device.address, name = %device.name, "bluetooth: disconnect succeeded");
        Ok(())
    }

    pub async fn pair(&self, address: &str) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            "bluetooth: pair requested"
        );
        let proxy = self.device_proxy(&device.path).await?;
        proxy
            .pair()
            .await
            .with_context(|| format!("failed to pair {}", device.address))?;
        tracing::info!(address = %device.address, name = %device.name, "bluetooth: pair succeeded");
        Ok(())
    }

    pub async fn trust(&self, address: &str, trusted: bool) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            trusted,
            action = trust_action(trusted),
            "bluetooth: trust requested"
        );
        let proxy = self.device_proxy(&device.path).await?;
        proxy
            .set_trusted(trusted)
            .await
            .with_context(|| format!("failed to set trust for {}", device.address))?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            trusted,
            action = trust_action(trusted),
            status = trust_status(trusted),
            "bluetooth: trust succeeded"
        );
        Ok(())
    }

    pub async fn forget(&self, address: &str) -> anyhow::Result<()> {
        let device = self.resolve_device(address).await?;
        tracing::info!(
            address = %device.address,
            name = %device.name,
            path = %device.path,
            adapter = %device.adapter_path,
            "bluetooth: forget requested"
        );
        let proxy = self.adapter_proxy(&device.adapter_path).await?;
        let device_path = ObjectPath::try_from(device.path.as_str())
            .map_err(|error| anyhow::anyhow!("{error}"))?;
        proxy
            .remove_device(device_path)
            .await
            .with_context(|| format!("failed to forget {}", device.address))?;
        tracing::info!(address = %device.address, name = %device.name, "bluetooth: forget succeeded");
        Ok(())
    }

    pub async fn start_discovery(&self) -> anyhow::Result<()> {
        let needs_bluez_call = {
            let mut discovery = self.discovery.lock().expect("bluetooth discovery mutex poisoned");
            discovery.start_popover()
        };

        if needs_bluez_call {
            if let Err(error) = self.raw_start_discovery().await {
                let mut discovery = self.discovery.lock().expect("bluetooth discovery mutex poisoned");
                discovery.rollback_popover();
                return Err(error);
            }
        } else {
            tracing::debug!("bluetooth: popover discovery claimed (already active)");
        }

        Ok(())
    }

    pub async fn stop_discovery(&self) -> anyhow::Result<()> {
        let needs_bluez_call = {
            let mut discovery = self.discovery.lock().expect("bluetooth discovery mutex poisoned");
            discovery.close_popover()
        };

        if needs_bluez_call {
            if let Err(error) = self.raw_stop_discovery_once().await {
                tracing::warn!(error = %error, "bluetooth: stop discovery failed");
                return Err(error);
            }
            tracing::info!("bluetooth: discovery stopped");
        } else {
            tracing::debug!("bluetooth: stop discovery skipped; no active sessions");
        }

        Ok(())
    }

    async fn begin_initial_discovery(&self) -> Option<tokio::time::Instant> {
        let needs_bluez_call = {
            let mut discovery = self.discovery.lock().expect("bluetooth discovery mutex poisoned");
            discovery.start_initial()
        };

        if needs_bluez_call {
            match self.raw_start_discovery().await {
                Ok(()) => {
                    tracing::info!(
                        seconds = INITIAL_DISCOVERY_WINDOW.as_secs(),
                        "bluetooth: initial discovery window started"
                    );
                }
                Err(error) => {
                    let mut discovery = self.discovery.lock().expect("bluetooth discovery mutex poisoned");
                    discovery.rollback_initial();
                    tracing::warn!(error = %error, "bluetooth: initial discovery start failed");
                    return None;
                }
            }
        } else {
            tracing::debug!("bluetooth: initial discovery claimed (already active)");
        }

        Some(tokio::time::Instant::now() + INITIAL_DISCOVERY_WINDOW)
    }

    async fn finish_initial_discovery(&self) -> anyhow::Result<()> {
        let needs_bluez_call = {
            let mut discovery = self.discovery.lock().expect("bluetooth discovery mutex poisoned");
            discovery.finish_initial()
        };

        if needs_bluez_call {
            self.raw_stop_discovery_once().await?;
            tracing::info!("bluetooth: initial discovery stopped");
        } else {
            tracing::debug!("bluetooth: initial discovery ended; popover still active");
        }

        Ok(())
    }

    async fn raw_start_discovery(&self) -> anyhow::Result<()> {
        let adapter_paths = self.adapter_paths().await?;
        if adapter_paths.is_empty() {
            tracing::info!("bluetooth: start discovery skipped; no adapters found");
            return Ok(());
        }

        tracing::info!(
            adapters = adapter_paths.len(),
            "bluetooth: start discovery requested"
        );

        for path in adapter_paths {
            let proxy = self.adapter_proxy(&path).await?;
            proxy
                .start_discovery()
                .await
                .with_context(|| format!("failed to start discovery on {path}"))?;
            tracing::debug!(path = %path, "bluetooth: discovery started on adapter");
        }

        Ok(())
    }

    async fn raw_stop_discovery_once(&self) -> anyhow::Result<()> {
        let adapter_paths = self.adapter_paths().await?;
        if adapter_paths.is_empty() {
            tracing::debug!("bluetooth: stop discovery skipped; no adapters found");
            return Ok(());
        }

        for path in &adapter_paths {
            let proxy = self.adapter_proxy(path).await?;
            proxy
                .stop_discovery()
                .await
                .with_context(|| format!("failed to stop discovery on {path}"))?;
            tracing::debug!(path = %path, "bluetooth: discovery stopped on adapter");
        }

        Ok(())
    }

    async fn properties_changed_stream(&self) -> anyhow::Result<MessageStream> {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.bluez")?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .build();

        MessageStream::for_match_rule(rule, &self.conn, None)
            .await
            .map_err(Into::into)
    }

    async fn adapter_paths(&self) -> anyhow::Result<Vec<String>> {
        let om = self.object_manager().await?;
        let objects = om
            .get_managed_objects()
            .await
            .context("failed to read BlueZ managed objects for adapters")?;
        let mut paths = objects
            .iter()
            .filter(|(_, interfaces)| interfaces.contains_key("org.bluez.Adapter1"))
            .map(|(path, _)| path.to_string())
            .collect::<Vec<_>>();
        paths.sort();
        Ok(paths)
    }

    async fn object_manager(&self) -> anyhow::Result<zbus::fdo::ObjectManagerProxy<'_>> {
        zbus::fdo::ObjectManagerProxy::builder(&self.conn)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn adapter_proxy<'a>(&self, path: &'a str) -> anyhow::Result<Adapter1Proxy<'a>> {
        Adapter1Proxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn device_proxy<'a>(&self, path: &'a str) -> anyhow::Result<Device1Proxy<'a>> {
        Device1Proxy::builder(&self.conn)
            .path(path)?
            .build()
            .await
            .map_err(Into::into)
    }

    async fn read_adapter(&self, path: &str) -> anyhow::Result<BluetoothAdapter> {
        let proxy = self.adapter_proxy(path).await?;
        Ok(BluetoothAdapter {
            path: path.to_owned(),
            name: proxy.alias().await.unwrap_or_default(),
            address: proxy.address().await.unwrap_or_default(),
            powered: proxy.powered().await.unwrap_or(false),
            discovering: proxy.discovering().await.unwrap_or(false),
        })
    }

    async fn read_device(&self, path: &str, has_battery: bool) -> anyhow::Result<BluetoothDevice> {
        let proxy = self.device_proxy(path).await?;
        let address = proxy.address().await.unwrap_or_default();
        let alias = proxy.alias().await.unwrap_or_default();
        let icon = proxy.icon().await.unwrap_or_default();
        let paired = proxy.paired().await.unwrap_or(false);
        let connected = proxy.connected().await.unwrap_or(false);
        let trusted = proxy.trusted().await.unwrap_or(false);
        let rssi = proxy.rssi().await.ok();
        let class = proxy.class().await.unwrap_or(0);
        let appearance = proxy.appearance().await.unwrap_or(0);
        let adapter = proxy
            .adapter()
            .await
            .map(|path| path.to_string())
            .unwrap_or_default();
        let battery = if has_battery {
            self.read_battery_percentage(path).await
        } else {
            None
        };

        let name = if !alias.is_empty() {
            alias
        } else if !address.is_empty() {
            address.clone()
        } else {
            "Unknown".into()
        };

        Ok(BluetoothDevice {
            address,
            name,
            device_type: BluetoothDeviceType::from_hints(appearance, class, &icon),
            paired,
            connected,
            trusted,
            battery,
            rssi,
            adapter,
        })
    }

    async fn read_battery_percentage(&self, path: &str) -> Option<u8> {
        let proxy = Battery1Proxy::builder(&self.conn)
            .path(path)
            .ok()?
            .build()
            .await
            .ok()?;
        proxy.percentage().await.ok()
    }

    async fn resolve_device(&self, address: &str) -> anyhow::Result<ResolvedDevice> {
        let om = self.object_manager().await?;
        let objects = om
            .get_managed_objects()
            .await
            .context("failed to read BlueZ managed objects for device lookup")?;

        for (path, interfaces) in &objects {
            let Some(props) = interfaces.get("org.bluez.Device1") else {
                continue;
            };

            let current_address = props
                .get("Address")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();
            if current_address != address {
                continue;
            }

            let name = props
                .get("Alias")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();
            let adapter_path = props
                .get("Adapter")
                .and_then(|v| {
                    zbus::zvariant::ObjectPath::try_from(v.clone())
                        .map(|p| p.to_string())
                        .ok()
                })
                .unwrap_or_default();

            return Ok(ResolvedDevice {
                path: path.to_string(),
                adapter_path,
                address: current_address,
                name: if name.is_empty() {
                    address.to_owned()
                } else {
                    name
                },
            });
        }

        bail!("unknown bluetooth device: {address}")
    }
}

struct ResolvedDevice {
    path: String,
    adapter_path: String,
    address: String,
    name: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DiscoveryClaims {
    initial: bool,
    popover: bool,
}

impl DiscoveryClaims {
    fn is_active(&self) -> bool {
        self.initial || self.popover
    }

    /// Register initial discovery claim. Returns true if BlueZ StartDiscovery
    /// needs to be called (no other claim was active).
    fn start_initial(&mut self) -> bool {
        if self.initial {
            return false;
        }
        let was_active = self.is_active();
        self.initial = true;
        !was_active
    }

    /// Register popover discovery claim. Returns true if BlueZ StartDiscovery
    /// needs to be called (no other claim was active).
    fn start_popover(&mut self) -> bool {
        if self.popover {
            return false;
        }
        let was_active = self.is_active();
        self.popover = true;
        !was_active
    }

    /// Release initial claim. Returns true if BlueZ StopDiscovery needs to
    /// be called (no claims remain).
    fn finish_initial(&mut self) -> bool {
        if !self.initial {
            return false;
        }
        self.initial = false;
        !self.is_active()
    }

    /// Release popover claim (and initial if still active, since the user
    /// explicitly closed the popover). Returns true if BlueZ StopDiscovery
    /// needs to be called (no claims remain).
    fn close_popover(&mut self) -> bool {
        if !self.popover && !self.initial {
            return false;
        }
        self.popover = false;
        self.initial = false;
        !self.is_active() // always true here, but kept for clarity
    }

    /// Roll back a failed popover start without affecting other claims.
    fn rollback_popover(&mut self) {
        self.popover = false;
    }

    /// Roll back a failed initial start without affecting other claims.
    fn rollback_initial(&mut self) {
        self.initial = false;
    }
}

fn merge_change_reason(
    current: Option<BluetoothChangeReason>,
    next: BluetoothChangeReason,
) -> BluetoothChangeReason {
    match current {
        None => next,
        Some(current) if current == next => current,
        Some(_) => BluetoothChangeReason::Mixed,
    }
}

fn is_bluez_properties_changed(message: &zbus::message::Message) -> bool {
    let header = message.header();

    let Some(member) = header.member() else {
        return false;
    };
    if member.as_str() != "PropertiesChanged" {
        return false;
    }

    let Some(interface) = header.interface() else {
        return false;
    };
    if interface.as_str() != "org.freedesktop.DBus.Properties" {
        return false;
    }

    let Some(path) = header.path() else {
        return false;
    };
    path.as_str().starts_with("/org/bluez")
}

fn trust_action(trusted: bool) -> &'static str {
    if trusted {
        "trust"
    } else {
        "untrust"
    }
}

fn trust_status(trusted: bool) -> &'static str {
    if trusted {
        "trusted"
    } else {
        "untrusted"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_popover_cancels_both_claims_and_needs_stop() {
        let mut claims = DiscoveryClaims {
            initial: true,
            popover: true,
        };

        assert!(claims.close_popover());
        assert_eq!(claims, DiscoveryClaims::default());
    }

    #[test]
    fn close_popover_noop_when_no_claims() {
        let mut claims = DiscoveryClaims::default();
        assert!(!claims.close_popover());
    }

    #[test]
    fn finish_initial_does_not_stop_when_popover_active() {
        let mut claims = DiscoveryClaims {
            initial: true,
            popover: true,
        };

        assert!(!claims.finish_initial());
        assert_eq!(
            claims,
            DiscoveryClaims {
                initial: false,
                popover: true,
            }
        );
    }

    #[test]
    fn finish_initial_stops_when_sole_claim() {
        let mut claims = DiscoveryClaims {
            initial: true,
            popover: false,
        };

        assert!(claims.finish_initial());
        assert_eq!(claims, DiscoveryClaims::default());
    }

    #[test]
    fn start_popover_skips_bluez_when_initial_active() {
        let mut claims = DiscoveryClaims {
            initial: true,
            popover: false,
        };

        assert!(!claims.start_popover());
        assert!(claims.popover);
    }

    #[test]
    fn start_popover_needs_bluez_when_nothing_active() {
        let mut claims = DiscoveryClaims::default();
        assert!(claims.start_popover());
        assert!(claims.popover);
    }

    fn adapter(path: &str, powered: bool, discovering: bool) -> BluetoothAdapter {
        BluetoothAdapter {
            path: path.to_owned(),
            name: "Adapter".into(),
            address: "00:11:22:33:44:55".into(),
            powered,
            discovering,
        }
    }

    fn device(address: &str, connected: bool, trusted: bool) -> BluetoothDevice {
        BluetoothDevice {
            address: address.to_owned(),
            name: "Device".into(),
            device_type: BluetoothDeviceType::Unknown,
            paired: true,
            connected,
            trusted,
            battery: Some(80),
            rssi: Some(-42),
            adapter: "/org/bluez/hci0".into(),
        }
    }

    #[test]
    fn snapshot_derives_status_from_adapters_and_devices() {
        let snapshot = BluetoothSnapshot::new(
            vec![
                adapter("/org/bluez/hci0", true, false),
                adapter("/org/bluez/hci1", false, true),
            ],
            vec![
                device("AA:BB:CC:DD:EE:FF", true, true),
                device("11:22:33:44:55:66", false, false),
            ],
        );

        assert_eq!(
            snapshot.status,
            BluetoothStatus {
                powered: true,
                discovering: true,
                connected_count: 1,
            }
        );
    }

    #[test]
    fn snapshot_counts_trusted_device_without_affecting_connected_count() {
        let snapshot = BluetoothSnapshot::new(
            vec![adapter("/org/bluez/hci0", true, false)],
            vec![
                device("AA:BB:CC:DD:EE:FF", false, true),
                device("11:22:33:44:55:66", true, false),
            ],
        );

        assert_eq!(snapshot.status.connected_count, 1);
        assert_eq!(
            snapshot.devices,
            vec![
                BluetoothDevice {
                    address: "11:22:33:44:55:66".into(),
                    name: "Device".into(),
                    device_type: BluetoothDeviceType::Unknown,
                    paired: true,
                    connected: true,
                    trusted: false,
                    battery: Some(80),
                    rssi: Some(-42),
                    adapter: "/org/bluez/hci0".into(),
                },
                BluetoothDevice {
                    address: "AA:BB:CC:DD:EE:FF".into(),
                    name: "Device".into(),
                    device_type: BluetoothDeviceType::Unknown,
                    paired: true,
                    connected: false,
                    trusted: true,
                    battery: Some(80),
                    rssi: Some(-42),
                    adapter: "/org/bluez/hci0".into(),
                },
            ]
        );
    }

    #[test]
    fn trust_helper_distinguishes_enable_and_disable_semantics() {
        assert_eq!(trust_action(true), "trust");
        assert_eq!(trust_action(false), "untrust");
        assert_eq!(trust_status(true), "trusted");
        assert_eq!(trust_status(false), "untrusted");
    }

    #[test]
    fn merge_change_reason_marks_bursts_as_mixed() {
        let reason = merge_change_reason(
            Some(BluetoothChangeReason::InterfacesAdded),
            BluetoothChangeReason::PropertiesChanged,
        );

        assert_eq!(reason, BluetoothChangeReason::Mixed);
    }

    #[test]
    fn device_type_prefers_classification_hints_before_icon_name() {
        let kind = BluetoothDeviceType::from_hints(0, 0x240418, "phone");

        assert_eq!(kind, BluetoothDeviceType::Headphones);
    }
}
