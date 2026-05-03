use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub enum LocationConfig {
    Static {
        latitude: f64,
        longitude: f64,
    },
    GeoClue,
    #[serde(rename = "ipapi")]
    IPAPI,
}

impl Default for LocationConfig {
    fn default() -> Self {
        Self::GeoClue
    }
}

impl std::fmt::Display for LocationConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static {
                latitude,
                longitude,
            } => write!(f, "static({latitude}, {longitude})"),
            Self::GeoClue => f.write_str("geoclue"),
            Self::IPAPI => f.write_str("ipapi"),
        }
    }
}
