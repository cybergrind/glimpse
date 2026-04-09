use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use relm4::gtk::{self, glib, prelude::*};
use relm4::{ComponentParts, ComponentSender, RelmApp, SimpleComponent};
use serde::Deserialize;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
enum WallpaperMode {
    #[default]
    Image,
    Directory,
    Color,
    Video,
    Schedule,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
enum ImageOrder {
    #[default]
    Sorted,
    Random,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
enum ContentFit {
    #[default]
    Fill,
    Contain,
    Cover,
}

impl ContentFit {
    fn to_gtk(&self) -> gtk::ContentFit {
        match self {
            ContentFit::Fill => gtk::ContentFit::Fill,
            ContentFit::Contain => gtk::ContentFit::Contain,
            ContentFit::Cover => gtk::ContentFit::Cover,
        }
    }
}

/// One entry in a `schedule` wallpaper — shows `path` starting at `time`.
#[derive(Debug, Clone, Deserialize)]
struct ScheduleFrame {
    /// 24-hour "HH:MM".
    time: String,
    path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct WallpaperConfig {
    mode: WallpaperMode,
    /// Image / directory / video path (required for those modes).
    path: Option<PathBuf>,
    /// Solid color in `#rrggbb` or `#rgb`; used in `color` mode and as fallback.
    color: String,
    /// Seconds between images (`directory` mode).
    interval_seconds: u64,
    /// Cycling order (`directory` mode).
    order: ImageOrder,
    /// How the image or video fills the screen.
    content_fit: ContentFit,
    /// Scan subdirectories (`directory` mode).
    recursive: bool,
    /// Loop the video (`video` mode, default true).
    looped: bool,
    /// Mute audio (`video` mode, default true).
    muted: bool,
    /// Time-keyed frames (`schedule` mode).
    frames: Vec<ScheduleFrame>,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            mode: WallpaperMode::Image,
            path: None,
            color: "#000000".to_owned(),
            interval_seconds: 300,
            order: ImageOrder::Sorted,
            content_fit: ContentFit::Fill,
            recursive: false,
            looped: true,
            muted: true,
            frames: Vec::new(),
        }
    }
}

// ── Component ─────────────────────────────────────────────────────────────────

struct WallpaperModel;

#[derive(Debug)]
enum WallpaperInput {}

#[relm4::component]
impl SimpleComponent for WallpaperModel {
    type Init = WallpaperConfig;
    type Input = WallpaperInput;
    type Output = ();

    view! {
        gtk::Window {
            set_visible: true,
            set_decorated: false,
            set_deletable: false,
            set_resizable: false,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        use gtk4_layer_shell::{Edge, Layer, LayerShell};
        root.init_layer_shell();
        root.set_layer(Layer::Background);
        root.set_namespace("glimpse-wallpaper");
        root.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::None);
        root.set_exclusive_zone(-1);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            root.set_anchor(edge, true);
        }

        root.set_child(Some(&build_wallpaper_widget(&init)));

        let model = WallpaperModel;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {}
    }
}

// ── Widget builders ───────────────────────────────────────────────────────────

fn build_wallpaper_widget(config: &WallpaperConfig) -> gtk::Widget {
    match &config.mode {
        WallpaperMode::Image => {
            let picture = make_picture(config.content_fit.to_gtk());
            match &config.path {
                Some(path) => set_wallpaper_image(path, &picture),
                None => warn!("wallpaper: mode=image requires 'path'"),
            }
            picture.upcast()
        }

        WallpaperMode::Directory => {
            let dir = match &config.path {
                Some(p) => p,
                None => {
                    warn!("wallpaper: mode=directory requires 'path', falling back to color");
                    return make_color_widget(&config.color);
                }
            };

            let mut paths = scan_directory_for_images(dir, config.recursive);
            if paths.is_empty() {
                warn!("wallpaper: no images in {}, falling back to color", dir.display());
                return make_color_widget(&config.color);
            }

            if matches!(config.order, ImageOrder::Random) {
                shuffle(&mut paths);
            }

            let picture = make_picture(config.content_fit.to_gtk());
            set_wallpaper_image(&paths[0], &picture);

            if paths.len() > 1 {
                let interval = config.interval_seconds as u32;
                let idx = Cell::new(0usize);
                let picture_clone = picture.clone();
                glib::timeout_add_seconds_local(interval, move || {
                    let next = (idx.get() + 1) % paths.len();
                    idx.set(next);
                    set_wallpaper_image(&paths[next], &picture_clone);
                    glib::ControlFlow::Continue
                });
            }

            picture.upcast()
        }

        WallpaperMode::Video => {
            let path = match &config.path {
                Some(p) => p,
                None => {
                    warn!("wallpaper: mode=video requires 'path', falling back to color");
                    return make_color_widget(&config.color);
                }
            };
            make_video_widget(path, config.content_fit.to_gtk(), config.looped, config.muted, &config.color)
        }

        WallpaperMode::Schedule => {
            let mut frames = config.frames.clone();
            if frames.is_empty() {
                warn!("wallpaper: mode=schedule requires [[wallpaper.frames]], falling back to color");
                return make_color_widget(&config.color);
            }
            frames.sort_by(|a, b| a.time.cmp(&b.time));
            make_schedule_widget(frames, config.content_fit.to_gtk())
        }

        WallpaperMode::Color => make_color_widget(&config.color),
    }
}

fn make_video_widget(
    path: &Path,
    content_fit: gtk::ContentFit,
    looped: bool,
    muted: bool,
    fallback_color: &str,
) -> gtk::Widget {
    use gstreamer::prelude::*;

    if let Err(e) = gstreamer::init() {
        warn!("wallpaper: GStreamer init failed: {e}, falling back to color");
        return make_color_widget(fallback_color);
    }

    let sink = match gstreamer::ElementFactory::make("gtk4paintablesink").build() {
        Ok(s) => s,
        Err(e) => {
            warn!("wallpaper: gtk4paintablesink unavailable: {e}");
            warn!("wallpaper: install gst-plugin-gtk4 (gst-plugins-rs), falling back to color");
            return make_color_widget(fallback_color);
        }
    };

    let paintable = sink.property::<gtk::gdk::Paintable>("paintable");

    let uri = format!("file://{}", path.display());
    let playbin = match gstreamer::ElementFactory::make("playbin")
        .property("uri", &uri)
        .property("video-sink", &sink)
        .build()
    {
        Ok(p) => p,
        Err(e) => {
            warn!("wallpaper: failed to build video pipeline: {e}, falling back to color");
            return make_color_widget(fallback_color);
        }
    };

    if muted {
        playbin.set_property("mute", true);
    }

    if looped {
        let bus = playbin.bus().unwrap();
        let pipeline = playbin.clone();
        let guard = bus.add_watch_local(move |_, msg| {
            if msg.type_() == gstreamer::MessageType::Eos {
                let _ = pipeline.seek_simple(
                    gstreamer::SeekFlags::FLUSH | gstreamer::SeekFlags::KEY_UNIT,
                    gstreamer::ClockTime::ZERO,
                );
            }
            glib::ControlFlow::Continue
        })
        .unwrap();
        std::mem::forget(guard);
    }

    if let Err(e) = playbin.set_state(gstreamer::State::Playing) {
        warn!("wallpaper: failed to start video pipeline: {e}");
    }

    // Pipeline must outlive the widget; this process runs forever.
    std::mem::forget(playbin);

    let picture = make_picture(content_fit);
    picture.set_paintable(Some(&paintable));
    info!("wallpaper: video {}", path.display());
    picture.upcast()
}

fn make_schedule_widget(frames: Vec<ScheduleFrame>, content_fit: gtk::ContentFit) -> gtk::Widget {
    let picture = make_picture(content_fit);

    // Show the correct frame immediately.
    let initial_idx = current_frame_index(&frames).unwrap_or(0);
    set_wallpaper_image(&frames[initial_idx].path, &picture);
    info!("wallpaper: schedule active frame '{}' ({})", frames[initial_idx].time, frames[initial_idx].path.display());

    // Poll every 60 s; swap only when the active frame index changes.
    let picture_clone = picture.clone();
    let last_idx = Cell::new(initial_idx);
    glib::timeout_add_seconds_local(60, move || {
        if let Some(idx) = current_frame_index(&frames) {
            if idx != last_idx.get() {
                last_idx.set(idx);
                set_wallpaper_image(&frames[idx].path, &picture_clone);
                info!("wallpaper: schedule switched to '{}' ({})", frames[idx].time, frames[idx].path.display());
            }
        }
        glib::ControlFlow::Continue
    });

    picture.upcast()
}

fn make_color_widget(color: &str) -> gtk::Widget {
    let (r, g, b) = parse_hex_color(color).unwrap_or_else(|| {
        warn!("wallpaper: invalid color '{color}', using black");
        (0.0, 0.0, 0.0)
    });
    let drawing = gtk::DrawingArea::new();
    drawing.set_hexpand(true);
    drawing.set_vexpand(true);
    drawing.set_draw_func(move |_, cr, _, _| {
        cr.set_source_rgb(r, g, b);
        let _ = cr.paint();
    });
    info!("wallpaper: solid color {color}");
    drawing.upcast()
}

// ── GTK helpers ───────────────────────────────────────────────────────────────

fn make_picture(content_fit: gtk::ContentFit) -> gtk::Picture {
    let picture = gtk::Picture::new();
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.set_halign(gtk::Align::Fill);
    picture.set_valign(gtk::Align::Fill);
    picture.set_content_fit(content_fit);
    picture
}

fn set_wallpaper_image(path: &Path, picture: &gtk::Picture) {
    if path.exists() {
        picture.set_file(Some(&gio::File::for_path(path)));
        info!("wallpaper: {}", path.display());
    } else {
        warn!("wallpaper: file not found: {}", path.display());
    }
}

fn parse_hex_color(color: &str) -> Option<(f64, f64, f64)> {
    let hex = color.trim_start_matches('#');
    let (r, g, b) = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()?;
            (r, g, b)
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b)
        }
        _ => return None,
    };
    Some((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0))
}

// ── Directory scan ────────────────────────────────────────────────────────────

fn scan_directory_for_images(dir: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut images = Vec::new();
    scan_dir_inner(dir, recursive, &mut images);
    images.sort();
    images
}

fn scan_dir_inner(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            warn!("wallpaper: failed to read {}: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                scan_dir_inner(&path, true, out);
            }
        } else if is_image_file(&path) {
            out.push(path);
        }
    }
}

fn is_image_file(path: &Path) -> bool {
    path.extension()
        .map(|ext| {
            matches!(
                ext.to_string_lossy().to_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "bmp" | "gif" | "tiff" | "webp" | "heic" | "heif"
            )
        })
        .unwrap_or(false)
}

fn shuffle(paths: &mut [PathBuf]) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xdeadbeef);
    let mut state = seed;
    for i in (1..paths.len()).rev() {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let j = (state >> 33) as usize % (i + 1);
        paths.swap(i, j);
    }
}

// ── Schedule helpers ──────────────────────────────────────────────────────────

/// Parse "HH:MM" into minutes-since-midnight. Returns `None` on invalid input.
fn parse_hhmm(time: &str) -> Option<u32> {
    let mut parts = time.splitn(2, ':');
    let h: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Returns the index of the frame that should be active right now.
///
/// Finds the last frame whose `time <= now`. If now is before all frames
/// (e.g. 02:00 with frames starting at 06:00), wraps to the last frame,
/// which represents the overnight carry-over from the previous day.
fn current_frame_index(frames: &[ScheduleFrame]) -> Option<usize> {
    use chrono::Timelike;
    let now = chrono::Local::now();
    let now_mins = now.hour() * 60 + now.minute();

    let idx = frames
        .iter()
        .enumerate()
        .filter(|(_, f)| parse_hhmm(&f.time).map(|t| t <= now_mins).unwrap_or(false))
        .map(|(i, _)| i)
        .last();

    Some(idx.unwrap_or(frames.len() - 1))
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let filter = EnvFilter::try_from_env("GLIMPSE_WALLPAPER_LOG_LEVEL")
        .unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    info!("starting glimpse-wallpaper");

    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("failed to load config: {e}");
            return;
        }
    };
    info!("wallpaper config: {:?}", config);

    RelmApp::new("me.aresa.GlimpseWallpaper").run::<WallpaperModel>(config);
}

fn load_config() -> Result<WallpaperConfig> {
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
