use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    Auto,
}
