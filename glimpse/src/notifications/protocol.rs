use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationEntry {
    pub id: u64,
    pub app_name: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationsServiceHealth {
    Starting,
    Ready,
    Reconnecting { attempt: u32 },
    Degraded { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationsActiveAction {
    SetDoNotDisturb(bool),
    Dismiss { id: u64 },
    ClearAll,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotificationsCommand {
    SetDoNotDisturb(bool),
    Dismiss { id: u64 },
    ClearAll,
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
