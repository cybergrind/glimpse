#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub address: String,
    pub id: String,
    pub title: String,
    pub status: Status,
    pub category: Category,
    pub item_is_menu: bool,
    pub menu_path: String,
    pub icon_theme_path: Option<String>,
    pub icon: Option<Icon>,
    pub overlay_icon: Option<Icon>,
    pub attention_icon: Option<Icon>,
    pub attention_movie_name: Option<String>,
    pub tooltip: Option<Tooltip>,
    pub menu: Vec<MenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Icon {
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
pub struct Tooltip {
    pub title: String,
    pub description: String,
    pub icon: Option<Icon>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub kind: MenuItemKind,
    pub icon: Option<Icon>,
    pub shortcut: Option<Vec<Vec<String>>>,
    pub toggle_type: MenuToggleType,
    pub toggle_state: MenuToggleState,
    pub children_display: Option<String>,
    pub disposition: MenuDisposition,
    pub children: Vec<MenuItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuItemKind {
    Separator,
    #[default]
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuToggleType {
    Checkmark,
    Radio,
    #[default]
    CannotBeToggled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuToggleState {
    On,
    Off,
    #[default]
    Indeterminate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MenuDisposition {
    #[default]
    Normal,
    Informative,
    Warning,
    Alert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Category {
    #[default]
    ApplicationStatus,
    Communications,
    SystemServices,
    Hardware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Unknown,
    Passive,
    Active,
    NeedsAttention,
}
