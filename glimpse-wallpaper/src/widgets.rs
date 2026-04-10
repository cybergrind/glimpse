//! GTK widget builders for each wallpaper mode.

use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
};

use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;
use gtk::{gdk, gio};
use tracing::{info, warn};

use crate::config::{
    ImageOrder, ScheduleFrame, WallpaperConfig, WallpaperMode, WorkspaceSlot,
};
use crate::heic;

// ── Top-level dispatcher ──────────────────────────────────────────────────────

pub fn build_wallpaper_widget(config: &WallpaperConfig) -> gtk::Widget {
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

            make_cycling_widget(paths, config.interval_seconds, config.content_fit.to_gtk(), config.transition_ms)
        }

        WallpaperMode::Schedule => {
            let mut frames = config.frames.clone();
            if frames.is_empty() {
                warn!("wallpaper: mode=schedule requires [[wallpaper.frames]], falling back to color");
                return make_color_widget(&config.color);
            }
            frames.sort_by(|a, b| a.time.cmp(&b.time));
            let paths: Vec<PathBuf> = frames.iter().map(|f| f.path.clone()).collect();
            let initial = current_schedule_index(&frames).unwrap_or(0);
            make_schedule_widget(frames, paths, initial, config.content_fit.to_gtk(), config.transition_ms)
        }

        WallpaperMode::Heic => {
            let path = match &config.path {
                Some(p) => p,
                None => {
                    warn!("wallpaper: mode=heic requires 'path', falling back to color");
                    return make_color_widget(&config.color);
                }
            };
            make_heic_widget(path, config.content_fit.to_gtk(), config.transition_ms, &config.color)
        }

        WallpaperMode::Color => make_color_widget(&config.color),

        WallpaperMode::Workspace => {
            warn!("wallpaper: mode=workspace is only valid inside [[wallpaper.monitors]], falling back to color");
            make_color_widget(&config.color)
        }
    }
}

// ── Workspace widget ──────────────────────────────────────────────────────────

/// Builds a `gtk::Stack` with one page per workspace slot.
///
/// Returns the stack widget and a closure that switches to the page for the
/// given 1-based workspace index. The niri event handler calls the closure
/// when a workspace becomes active.
pub fn make_workspace_widget(
    slots: &[WorkspaceSlot],
    fallback_color: &str,
) -> (gtk::Widget, Rc<dyn Fn(u8)>) {
    if slots.is_empty() {
        warn!("wallpaper: mode=workspace has no [[workspaces]] entries, falling back to color");
        let widget = make_color_widget(fallback_color);
        let switch: Rc<dyn Fn(u8)> = Rc::new(|_| {});
        return (widget, switch);
    }

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(600);
    stack.set_hexpand(true);
    stack.set_vexpand(true);

    for slot in slots {
        let child = build_wallpaper_widget(&slot.config);
        let page_name = slot.index.to_string();
        stack.add_named(&child, Some(&page_name));
    }

    // Show the first slot immediately without animation.
    let first_name = slots[0].index.to_string();
    stack.set_transition_type(gtk::StackTransitionType::None);
    stack.set_visible_child_name(&first_name);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    let stack_clone = stack.clone();
    let switch: Rc<dyn Fn(u8)> = Rc::new(move |idx: u8| {
        let name = idx.to_string();
        if stack_clone.child_by_name(&name).is_some() {
            stack_clone.set_visible_child_name(&name);
            info!("wallpaper: workspace → {idx}");
        }
    });

    (stack.upcast(), switch)
}

// ── Cycling widget (directory mode) ──────────────────────────────────────────

fn make_cycling_widget(
    paths: Vec<PathBuf>,
    interval_seconds: u64,
    content_fit: gtk::ContentFit,
    transition_ms: u32,
) -> gtk::Widget {
    let (widget, show) = make_fader(content_fit, transition_ms);
    show(&paths[0]);

    if paths.len() > 1 {
        let idx = Cell::new(0usize);
        let interval = interval_seconds as u32;
        glib::timeout_add_seconds_local(interval, move || {
            let next = (idx.get() + 1) % paths.len();
            idx.set(next);
            show(&paths[next]);
            glib::ControlFlow::Continue
        });
    }

    widget
}

// ── Schedule widget ───────────────────────────────────────────────────────────

fn make_schedule_widget(
    frames: Vec<ScheduleFrame>,
    paths: Vec<PathBuf>,
    initial: usize,
    content_fit: gtk::ContentFit,
    transition_ms: u32,
) -> gtk::Widget {
    let (widget, show) = make_fader(content_fit, transition_ms);
    show(&paths[initial]);
    info!(
        "wallpaper: schedule active '{}' ({})",
        frames[initial].time,
        paths[initial].display()
    );

    let last_idx = Cell::new(initial);
    glib::timeout_add_seconds_local(60, move || {
        if let Some(idx) = current_schedule_index(&frames) {
            if idx != last_idx.get() {
                last_idx.set(idx);
                show(&paths[idx]);
                info!("wallpaper: schedule → '{}' ({})", frames[idx].time, paths[idx].display());
            }
        }
        glib::ControlFlow::Continue
    });

    widget
}

// ── HEIC widget ───────────────────────────────────────────────────────────────

fn make_heic_widget(
    path: &Path,
    content_fit: gtk::ContentFit,
    transition_ms: u32,
    fallback_color: &str,
) -> gtk::Widget {
    let unpacked = match heic::unpack(path) {
        Ok(u) => u,
        Err(e) => {
            warn!("wallpaper: failed to unpack HEIC {}: {e}", path.display());
            return make_color_widget(fallback_color);
        }
    };

    let frames = unpacked.frames;
    if frames.is_empty() {
        warn!("wallpaper: no frames in HEIC {}", path.display());
        return make_color_widget(fallback_color);
    }

    let frame_count = frames.len();
    let schedule_items: Vec<(usize, f64)> = match unpacked.schedule {
        heic::HeicSchedule::H24(ref items) => {
            items.iter().map(|it| (it.index, it.time)).collect()
        }
        heic::HeicSchedule::Appearance { light, .. } => vec![(light, 0.0)],
    };

    let initial = h24_frame_from_items(&schedule_items, frame_count);
    let (widget, show) = make_fader(content_fit, transition_ms);
    show(&frames[initial]);
    info!("wallpaper: heic frame {} of {}", initial, frame_count);

    let last_idx = Cell::new(initial);
    glib::timeout_add_seconds_local(60, move || {
        let idx = h24_frame_from_items(&schedule_items, frame_count);
        if idx != last_idx.get() {
            last_idx.set(idx);
            show(&frames[idx]);
            info!("wallpaper: heic → frame {idx}");
        }
        glib::ControlFlow::Continue
    });

    widget
}

fn h24_frame_from_items(items: &[(usize, f64)], frame_count: usize) -> usize {
    use chrono::Timelike;
    let now = chrono::Local::now();
    let frac = (now.hour() * 3600 + now.minute() * 60 + now.second()) as f64 / 86400.0;
    items
        .iter()
        .min_by_key(|(_, t)| {
            let d = (t - frac).abs();
            let d = d.min(1.0 - d);
            (d * 1_000_000_000.0) as u64
        })
        .map(|(idx, _)| *idx)
        .unwrap_or(0)
        .min(frame_count.saturating_sub(1))
}

// ── Color widget ──────────────────────────────────────────────────────────────

pub fn make_color_widget(color: &str) -> gtk::Widget {
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

// ── Crossfade fader ───────────────────────────────────────────────────────────

/// Returns `(widget, show_fn)`.
///
/// `widget` is the root GTK widget to place in the window.
/// `show_fn` swaps to a new image, crossfading if `transition_ms > 0`.
/// The first call always loads the initial image instantly (no fade from black).
pub fn make_fader(
    content_fit: gtk::ContentFit,
    transition_ms: u32,
) -> (gtk::Widget, Rc<dyn Fn(&Path)>) {
    if transition_ms == 0 {
        let picture = make_picture(content_fit);
        let picture_clone = picture.clone();
        let show: Rc<dyn Fn(&Path)> = Rc::new(move |path: &Path| {
            set_wallpaper_image(path, &picture_clone);
        });
        (picture.upcast(), show)
    } else {
        // A/B crossfade via gtk::Stack.
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_transition_duration(transition_ms);
        stack.set_hexpand(true);
        stack.set_vexpand(true);

        let pic_a = make_picture(content_fit);
        let pic_b = make_picture(content_fit);
        stack.add_named(&pic_a, Some("a"));
        stack.add_named(&pic_b, Some("b"));
        stack.set_visible_child_name("a");

        let on_b = Rc::new(Cell::new(false));
        let first = Rc::new(Cell::new(true));
        let stack_clone = stack.clone();
        let on_b_clone = on_b.clone();

        let show: Rc<dyn Fn(&Path)> = Rc::new(move |path: &Path| {
            if first.get() {
                // Suppress the crossfade animation for the initial image so
                // the screen is covered immediately at startup.
                first.set(false);
                set_wallpaper_image(path, &pic_a);
                stack_clone.set_transition_type(gtk::StackTransitionType::None);
                stack_clone.set_visible_child_name("a");
                stack_clone.set_transition_type(gtk::StackTransitionType::Crossfade);
                return;
            }
            if on_b_clone.get() {
                set_wallpaper_image(path, &pic_a);
                stack_clone.set_visible_child_name("a");
            } else {
                set_wallpaper_image(path, &pic_b);
                stack_clone.set_visible_child_name("b");
            }
            on_b_clone.set(!on_b_clone.get());
        });

        (stack.upcast(), show)
    }
}

// ── GTK primitives ────────────────────────────────────────────────────────────

pub fn make_picture(content_fit: gtk::ContentFit) -> gtk::Picture {
    let picture = gtk::Picture::new();
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.set_halign(gtk::Align::Fill);
    picture.set_valign(gtk::Align::Fill);
    picture.set_content_fit(content_fit);
    picture.set_can_shrink(true);
    picture
}

pub fn set_wallpaper_image(path: &Path, picture: &gtk::Picture) {
    if !path.exists() {
        warn!("wallpaper: file not found: {}", path.display());
        return;
    }

    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Use the `image` crate for formats that often lack a system gdk-pixbuf loader.
    // Load synchronously so the picture has the correct natural size immediately.
    // Async loading (set_file) causes layout to be committed while the image is
    // still loading (natural size = 0), which breaks content_fit scaling.
    let result: anyhow::Result<gdk::Paintable> = if matches!(ext.as_str(), "webp" | "heic" | "heif") {
        load_via_image_crate(path).map(|t| t.upcast())
    } else {
        gdk::Texture::from_file(&gio::File::for_path(path))
            .map(|t| t.upcast())
            .map_err(|e| anyhow::anyhow!(e))
    };

    match result {
        Ok(paintable) => {
            picture.set_paintable(Some(&paintable));
            info!("wallpaper: {}", path.display());
        }
        Err(e) => {
            warn!("wallpaper: failed to load {}: {e}", path.display());
        }
    }
}

fn load_via_image_crate(path: &Path) -> anyhow::Result<gdk::MemoryTexture> {
    let img = image::open(path)?.into_rgba8();
    let (width, height) = img.dimensions();
    let bytes = glib::Bytes::from(img.as_raw().as_slice());
    let texture = gdk::MemoryTexture::new(
        width as i32,
        height as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        (width * 4) as usize,
    );
    Ok(texture)
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

pub fn scan_directory_for_images(dir: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut images = Vec::new();
    scan_dir_inner(dir, recursive, &mut images);
    images.sort();
    images
}

fn scan_dir_inner(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
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

fn parse_hhmm(time: &str) -> Option<u32> {
    let mut parts = time.splitn(2, ':');
    let h: u32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

pub fn current_schedule_index(frames: &[ScheduleFrame]) -> Option<usize> {
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
