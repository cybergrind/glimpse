use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SessionConfig {
    pub show_lock: bool,
    pub show_logout: bool,
    pub show_suspend: bool,
    pub show_hibernate: bool,
    pub show_reboot: bool,
    pub show_shutdown: bool,
    pub confirm_logout: bool,
    pub confirm_suspend: bool,
    pub confirm_hibernate: bool,
    pub confirm_reboot: bool,
    pub confirm_shutdown: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            show_lock: true,
            show_logout: true,
            show_suspend: true,
            show_hibernate: false,
            show_reboot: true,
            show_shutdown: true,
            confirm_logout: true,
            confirm_suspend: true,
            confirm_hibernate: true,
            confirm_reboot: true,
            confirm_shutdown: true,
        }
    }
}
