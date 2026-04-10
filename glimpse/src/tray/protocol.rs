#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraySnapshot {
    pub items: Vec<TrayItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

impl Default for TrayServiceHealth {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayServiceState {
    pub health: TrayServiceHealth,
    pub snapshot: TraySnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayServiceCommand {
    Activate { address: String, x: i32, y: i32 },
    OpenContextMenu { address: String, x: i32, y: i32 },
    AboutToShowMenu { address: String, menu_path: String, item_id: i32 },
    ActivateMenuItem { address: String, menu_path: String, submenu_id: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItem {
    pub address: String,
    pub id: String,
    pub title: String,
    pub status: TrayStatus,
    pub category: TrayCategory,
    pub item_is_menu: bool,
    pub menu_path: String,
    pub icon_theme_path: Option<String>,
    pub icon: Option<TrayIcon>,
    pub overlay_icon: Option<TrayIcon>,
    pub attention_icon: Option<TrayIcon>,
    pub attention_movie_name: Option<String>,
    pub tooltip: Option<TrayTooltip>,
    pub menu: Vec<TrayMenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayIcon {
    Name(String),
    FilePath(String),
    Pixmap {
        width: i32,
        height: i32,
        pixels: Vec<u8>,
    },
    EncodedBytes(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayTooltip {
    pub title: String,
    pub description: String,
    pub icon: Option<TrayIcon>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub kind: TrayMenuItemKind,
    pub icon: Option<TrayIcon>,
    pub shortcut: Option<Vec<Vec<String>>>,
    pub toggle_type: TrayMenuToggleType,
    pub toggle_state: TrayMenuToggleState,
    pub children_display: Option<String>,
    pub disposition: TrayMenuDisposition,
    pub children: Vec<TrayMenuItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayMenuItemKind {
    Separator,
    #[default]
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayMenuToggleType {
    Checkmark,
    Radio,
    #[default]
    CannotBeToggled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayMenuToggleState {
    On,
    Off,
    #[default]
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayMenuDisposition {
    #[default]
    Normal,
    Informative,
    Warning,
    Alert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayCategory {
    #[default]
    ApplicationStatus,
    Communications,
    SystemServices,
    Hardware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrayStatus {
    #[default]
    Unknown,
    Passive,
    Active,
    NeedsAttention,
}
