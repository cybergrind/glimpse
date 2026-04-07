use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MprisConfig {
    pub label_format: String,
    pub show_artwork: bool,
    pub hide_when_empty: bool,
    pub max_rows: usize,
}

impl Default for MprisConfig {
    fn default() -> Self {
        Self {
            label_format: "{artist} - {track}".into(),
            show_artwork: true,
            hide_when_empty: true,
            max_rows: 6,
        }
    }
}
