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
    pub actions: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    pub timestamp: u64,
    pub resident: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationsServiceHealth {
    Starting,
    Ready,
    Degraded { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationsActiveAction {
    Dismiss { id: u32 },
    DismissAll,
    InvokeAction { id: u32, action_key: String },
    SetDnd(bool),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationsCommand {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationsServiceState {
    pub health: NotificationsServiceHealth,
    pub notifications: Vec<NotificationEntry>,
    pub dnd: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_action: Option<NotificationsActiveAction>,
}

impl Default for NotificationsServiceState {
    fn default() -> Self {
        Self {
            health: NotificationsServiceHealth::Starting,
            notifications: Vec::new(),
            dnd: false,
            active_action: None,
        }
    }
}
