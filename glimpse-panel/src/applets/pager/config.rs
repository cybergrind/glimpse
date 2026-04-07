use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PagerStyle {
    Pills,
    Numbered,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ScrollAction {
    Windows,
    Workspaces,
}

fn default_style() -> PagerStyle {
    PagerStyle::Pills
}

fn default_count() -> u32 {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct PagerConfig {
    #[serde(default = "default_style")]
    pub style: PagerStyle,
    #[serde(default = "default_count")]
    pub count: u32,
    #[serde(default)]
    pub scroll_action: Option<ScrollAction>,
}

impl Default for PagerConfig {
    fn default() -> Self {
        Self {
            style: default_style(),
            count: default_count(),
            scroll_action: None,
        }
    }
}
