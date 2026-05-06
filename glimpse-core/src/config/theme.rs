use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    Auto,
}
