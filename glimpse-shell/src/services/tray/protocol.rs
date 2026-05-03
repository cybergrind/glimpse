use super::model::Snapshot;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Health {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

impl Default for Health {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub health: Health,
    pub snapshot: Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Activate {
        address: String,
        x: i32,
        y: i32,
    },
    SecondaryActivate {
        address: String,
        x: i32,
        y: i32,
    },
    OpenContextMenu {
        address: String,
        x: i32,
        y: i32,
    },
    Scroll {
        address: String,
        delta: i32,
        orientation: ScrollOrientation,
    },
    AboutToShowMenu {
        address: String,
        menu_path: String,
        item_id: i32,
    },
    ActivateMenuItem {
        address: String,
        menu_path: String,
        item_id: i32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOrientation {
    Horizontal,
    Vertical,
}

impl ScrollOrientation {
    pub fn as_dbus_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }
}
