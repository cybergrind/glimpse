use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspacesConfig {
    #[serde(default = "default_style")]
    pub style: WorkspacesStyle,
    #[serde(default = "default_count")]
    pub count: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspacesStyle {
    Pills,
    Numbered,
}

fn default_style() -> WorkspacesStyle {
    WorkspacesStyle::Pills
}

fn default_count() -> u32 {
    10
}

impl Default for WorkspacesConfig {
    fn default() -> Self {
        Self {
            style: default_style(),
            count: default_count(),
        }
    }
}
