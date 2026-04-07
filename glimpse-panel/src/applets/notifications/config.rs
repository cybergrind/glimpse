use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct NotificationsConfig {
    pub popup_position: String,
    pub popup_margin_top: i32,
    pub popup_timeout: u32,
    pub history_limit: u32,
    pub show_popup: bool,
    /// "count" = number badge (max 9+), "dot" = accent dot, "" = no badge
    pub badge_style: String,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            popup_position: "top-center".into(),
            popup_margin_top: 12,
            popup_timeout: 5000,
            history_limit: 100,
            show_popup: true,
            badge_style: "dot".into(),
        }
    }
}
