use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationEntry {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_entry: Option<String>,
    pub summary: String,
    pub body: String,
    pub urgency: u8,
    pub actions: Vec<NotificationAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub timestamp: u64,
    pub resident: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationAction {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Health {
    Starting,
    Ready,
    Degraded(String),
}

impl Default for Health {
    fn default() -> Self {
        Self::Starting
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActiveAction {
    Dismiss { id: u32 },
    DismissAll,
    InvokeAction { id: u32, action_key: String },
    SetDnd(bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Dismiss {
        id: u32,
    },
    DismissAll,
    InvokeAction {
        id: u32,
        action_key: String,
        activation_token: Option<String>,
    },
    SetDnd(bool),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub health: Health,
    pub notifications: Vec<NotificationEntry>,
    pub dnd: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_action: Option<ActiveAction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    NotificationClosed { id: u32, reason: u32 },
    ActionInvoked { id: u32, action_key: String },
    ActivationToken { id: u32, token: String },
}
