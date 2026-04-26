#![allow(dead_code)]

use std::fmt;

use serde::Serialize;

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
    pub discoverable: bool,
    pub pairable: bool,
    pub address_type: String,
    pub class: u32,
    pub discoverable_timeout: u32,
    pub pairable_timeout: u32,
    pub modalias: String,
    pub roles: Vec<String>,
    pub uuids: Vec<String>,
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
            Self::Display | Self::VideoMonitor | Self::VideoConferencing | Self::SetTopBox => {
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
    pub path: String,
    pub address: String,
    pub alias: String,
    pub name: String,
    pub device_type: BluetoothDeviceType,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    pub battery: Option<u8>,
    pub rssi: Option<i16>,
    pub class: u32,
    pub appearance: u16,
    pub adapter: String,
}

pub fn device_display_name(alias: &str, address: &str) -> String {
    if !alias.is_empty() {
        alias.to_owned()
    } else if !address.is_empty() {
        address.to_owned()
    } else {
        "Unknown".into()
    }
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct BluetoothSnapshot {
    pub status: BluetoothStatus,
    pub adapters: Vec<BluetoothAdapter>,
    pub devices: Vec<BluetoothDevice>,
}

impl BluetoothSnapshot {
    pub fn new(mut adapters: Vec<BluetoothAdapter>, mut devices: Vec<BluetoothDevice>) -> Self {
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
pub enum BluezEvent {
    Changed { reason: BluetoothChangeReason },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(path: &str, powered: bool, discovering: bool) -> BluetoothAdapter {
        BluetoothAdapter {
            path: path.into(),
            name: String::new(),
            address: String::new(),
            powered,
            discovering,
            discoverable: false,
            pairable: false,
            address_type: String::new(),
            class: 0,
            discoverable_timeout: 0,
            pairable_timeout: 0,
            modalias: String::new(),
            roles: Vec::new(),
            uuids: Vec::new(),
        }
    }

    fn device(address: &str, connected: bool, trusted: bool) -> BluetoothDevice {
        BluetoothDevice {
            path: format!("/org/bluez/hci0/dev_{}", address.replace(':', "_")),
            address: address.into(),
            alias: address.into(),
            name: String::new(),
            device_type: BluetoothDeviceType::Unknown,
            paired: false,
            connected,
            trusted,
            battery: None,
            rssi: None,
            class: 0,
            appearance: 0,
            adapter: "/org/bluez/hci0".into(),
        }
    }

    #[test]
    fn snapshot_sorts_and_derives_status() {
        let snapshot = BluetoothSnapshot::new(
            vec![
                adapter("/org/bluez/hci1", false, true),
                adapter("/org/bluez/hci0", true, false),
            ],
            vec![
                device("BB:00", false, false),
                device("AA:00", true, false),
                device("CC:00", true, true),
            ],
        );

        assert_eq!(snapshot.adapters[0].path, "/org/bluez/hci0");
        assert_eq!(snapshot.devices[0].address, "AA:00");
        assert_eq!(
            snapshot.status,
            BluetoothStatus {
                powered: true,
                discovering: true,
                connected_count: 2,
            }
        );
    }

    #[test]
    fn device_type_uses_class_before_icon_hint() {
        let kind = BluetoothDeviceType::from_hints(0, 0x240418, "phone");

        assert_eq!(kind, BluetoothDeviceType::Headphones);
        assert_eq!(kind.icon(true), "audio-headphones-symbolic");
        assert_eq!(kind.label(), "Headphones");
    }

    #[test]
    fn change_reason_display_is_stable() {
        assert_eq!(
            BluetoothChangeReason::InterfacesAdded.to_string(),
            "interfaces-added"
        );
        assert_eq!(BluetoothChangeReason::Mixed.to_string(), "mixed");
    }

    #[test]
    fn device_display_name_prefers_alias_then_address() {
        assert_eq!(device_display_name("Headphones", "AA:BB"), "Headphones");
        assert_eq!(device_display_name("", "AA:BB"), "AA:BB");
        assert_eq!(device_display_name("", ""), "Unknown");
    }
}
