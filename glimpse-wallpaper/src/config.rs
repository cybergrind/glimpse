//! Configuration types and loading for glimpse-wallpaper.

use std::{
    fs,
    path::PathBuf,
};

use anyhow::{Context, Result};
use gtk4 as gtk;
use serde::Deserialize;
use tracing::info;

// ── Enums ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WallpaperMode {
    #[default]
    Image,
    Directory,
    Color,
    Schedule,
    Heic,
    Workspace,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ImageOrder {
    #[default]
    Sorted,
    Random,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ContentFit {
    Fill,
    Contain,
    #[default]
    Cover,
}

impl ContentFit {
    pub fn to_gtk(&self) -> gtk::ContentFit {
        match self {
            ContentFit::Fill => gtk::ContentFit::Fill,
            ContentFit::Contain => gtk::ContentFit::Contain,
            ContentFit::Cover => gtk::ContentFit::Cover,
        }
    }
}

// ── Leaf types ────────────────────────────────────────────────────────────────

/// One entry in a `schedule` wallpaper — shows `path` starting at `time`.
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleFrame {
    /// 24-hour "HH:MM".
    pub time: String,
    pub path: PathBuf,
}

// ── Core config ───────────────────────────────────────────────────────────────

/// Wallpaper settings shared by the root config and per-workspace slots.
///
/// Fields without a value in TOML fall back to their defaults.
/// When embedded inside [`WorkspaceSlot`], `monitors` is always empty.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct WallpaperConfig {
    pub mode: WallpaperMode,
    /// Image / directory / HEIC path.
    pub path: Option<PathBuf>,
    /// Solid color in `#rrggbb` or `#rgb`; also used as fallback.
    pub color: String,
    /// Seconds between images (`directory` mode).
    pub interval_seconds: u64,
    /// Cycling order (`directory` mode).
    pub order: ImageOrder,
    /// How the image fills the screen.
    pub content_fit: ContentFit,
    /// Scan subdirectories (`directory` mode).
    pub recursive: bool,
    /// Crossfade duration in ms between images. 0 = instant cut.
    pub transition_ms: u32,
    /// Time-keyed frames (`schedule` mode).
    pub frames: Vec<ScheduleFrame>,
    /// Per-monitor overrides (root config only; ignored inside workspace slots).
    pub monitors: Vec<MonitorConfig>,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            mode: WallpaperMode::Image,
            path: None,
            color: "#000000".to_owned(),
            interval_seconds: 300,
            order: ImageOrder::Sorted,
            content_fit: ContentFit::Cover,
            recursive: false,
            transition_ms: 800,
            frames: Vec::new(),
            monitors: Vec::new(),
        }
    }
}

// ── Per-monitor config ────────────────────────────────────────────────────────

/// Per-monitor config override. All fields except `name` are optional;
/// unset fields fall back to the root [`WallpaperConfig`].
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MonitorConfig {
    /// Connector name to match, e.g. `"DP-1"`, `"HDMI-1"`. Empty = never matches.
    pub name: String,
    pub mode: Option<WallpaperMode>,
    pub path: Option<PathBuf>,
    pub color: Option<String>,
    pub interval_seconds: Option<u64>,
    pub order: Option<ImageOrder>,
    pub content_fit: Option<ContentFit>,
    pub recursive: Option<bool>,
    pub transition_ms: Option<u32>,
    pub frames: Option<Vec<ScheduleFrame>>,
    /// Workspace slots for `mode = "workspace"`.
    pub workspaces: Vec<WorkspaceSlot>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            mode: None,
            path: None,
            color: None,
            interval_seconds: None,
            order: None,
            content_fit: None,
            recursive: None,
            transition_ms: None,
            frames: None,
            workspaces: Vec::new(),
        }
    }
}

/// One workspace slot in `mode = "workspace"`.
///
/// `index` is the 1-based workspace position on the output.
/// All remaining fields are a full [`WallpaperConfig`] (flattened); the
/// `monitors` key is ignored if present.
#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceSlot {
    pub index: u8,
    #[serde(flatten)]
    pub config: WallpaperConfig,
}

// ── Config resolution ─────────────────────────────────────────────────────────

/// Merge a [`MonitorConfig`] override on top of the root config.
///
/// If no monitor entry matches `connector`, returns a clone of `root`.
pub fn resolve_config(root: &WallpaperConfig, connector: &str) -> WallpaperConfig {
    let Some(ov) = root.monitors.iter().find(|m| m.name == connector) else {
        return root.clone();
    };
    WallpaperConfig {
        mode: ov.mode.clone().unwrap_or_else(|| root.mode.clone()),
        path: ov.path.clone().or_else(|| root.path.clone()),
        color: ov.color.clone().unwrap_or_else(|| root.color.clone()),
        interval_seconds: ov.interval_seconds.unwrap_or(root.interval_seconds),
        order: ov.order.clone().unwrap_or_else(|| root.order.clone()),
        content_fit: ov.content_fit.clone().unwrap_or_else(|| root.content_fit.clone()),
        recursive: ov.recursive.unwrap_or(root.recursive),
        transition_ms: ov.transition_ms.unwrap_or(root.transition_ms),
        frames: ov.frames.clone().unwrap_or_else(|| root.frames.clone()),
        monitors: Vec::new(),
    }
}

// ── Loading ───────────────────────────────────────────────────────────────────

pub fn config_file_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("glimpse").join("config.toml"))
}

pub fn load_config() -> Result<WallpaperConfig> {
    let config_path = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine config directory"))?
        .join("glimpse")
        .join("config.toml");

    info!("loading config from {}", config_path.display());

    let content = fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;

    #[derive(Deserialize)]
    struct Root {
        wallpaper: WallpaperConfig,
    }

    let root: Root = toml::from_str(&content).context("failed to parse config")?;
    Ok(root.wallpaper)
}
