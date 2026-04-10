//! Apple Dynamic Desktop (HEIC/HEIF) support.
//!
//! Decodes multi-frame HEIC wallpapers, parses Apple's XMP schedule metadata
//! (H24 time-based or Solar sun-position-based), caches extracted PNG frames
//! on disk, and returns the correct frame index for the current moment.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use base64::Engine;
use tracing::{debug, info, warn};

// ── Public types ──────────────────────────────────────────────────────────────

/// A fully unpacked HEIC wallpaper ready for display.
pub struct UnpackedHeic {
    /// Extracted frame PNGs on disk, indexed from 0.
    pub frames: Vec<PathBuf>,
    /// How to pick the current frame.
    pub schedule: HeicSchedule,
}

pub enum HeicSchedule {
    /// Time-of-day schedule — `t` is a fraction of 24 h (0.0 = midnight, 1.0 = next midnight).
    H24(Vec<TimeItem>),
    /// Dark/light appearance index pair with no time component.
    Appearance { light: usize },
}

pub struct TimeItem {
    pub index: usize,
    pub time: f64,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Open a HEIC file, extract all frames to a disk cache, parse the XMP schedule,
/// and return an [`UnpackedHeic`] ready for display.
pub fn unpack(path: &Path) -> Result<UnpackedHeic> {
    let cache_dir = frame_cache_dir(path)?;
    let frames = if cache_is_valid(&cache_dir) {
        debug!("heic: cache hit at {}", cache_dir.display());
        collect_cached_frames(&cache_dir)
    } else {
        info!("heic: extracting frames from {} → {}", path.display(), cache_dir.display());
        std::fs::create_dir_all(&cache_dir)?;
        extract_frames(path, &cache_dir)?
    };

    if frames.is_empty() {
        bail!("heic: no frames extracted from {}", path.display());
    }

    let schedule = read_schedule(path).unwrap_or_else(|e| {
        warn!("heic: failed to read XMP schedule ({e}), defaulting to frame 0");
        HeicSchedule::Appearance { light: 0 }
    });

    Ok(UnpackedHeic { frames, schedule })
}

// ── Frame cache ───────────────────────────────────────────────────────────────

fn frame_cache_dir(path: &Path) -> Result<PathBuf> {
    let cache_root = dirs::cache_dir()
        .context("cannot determine cache directory")?
        .join("glimpse")
        .join("wallpapers");

    // Use seahash of the file bytes as the cache key.
    let bytes = std::fs::read(path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let hash = seahash::hash(&bytes);
    Ok(cache_root.join(format!("{hash:016x}")))
}

fn cache_is_valid(dir: &Path) -> bool {
    dir.exists()
        && std::fs::read_dir(dir)
            .map(|mut e| e.next().is_some())
            .unwrap_or(false)
}

fn collect_cached_frames(dir: &Path) -> Vec<PathBuf> {
    let mut frames: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "png").unwrap_or(false))
        .collect();
    frames.sort();
    frames
}

// ── Frame extraction ──────────────────────────────────────────────────────────

fn extract_frames(heic_path: &Path, cache_dir: &Path) -> Result<Vec<PathBuf>> {
    use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

    let ctx = HeifContext::read_from_file(
        heic_path.to_str().context("non-UTF-8 path")?,
    )?;

    let lib = LibHeif::new();
    let ids = ctx.image_ids();

    let mut paths = Vec::with_capacity(ids.len());

    for (seq, id) in ids.iter().enumerate() {
        let id = *id;
        let handle = match ctx.image_handle(id) {
            Ok(h) => h,
            Err(e) => {
                warn!("heic: skipping frame {seq}: {e}");
                continue;
            }
        };

        let image = match lib.decode(&handle, ColorSpace::Rgb(RgbChroma::Rgb), None) {
            Ok(img) => img,
            Err(e) => {
                warn!("heic: failed to decode frame {seq}: {e}");
                continue;
            }
        };

        let plane = match image.planes().interleaved {
            Some(p) => p,
            None => {
                warn!("heic: frame {seq} has no interleaved plane");
                continue;
            }
        };

        let w = handle.width();
        let h = handle.height();

        // Remove stride padding — copy each row's valid pixels.
        let row_bytes = (w * 3) as usize;
        let mut packed = Vec::with_capacity(row_bytes * h as usize);
        for row in 0..h as usize {
            let start = row * plane.stride;
            packed.extend_from_slice(&plane.data[start..start + row_bytes]);
        }

        let img = image::ImageBuffer::<image::Rgb<u8>, _>::from_raw(w, h, packed)
            .context("invalid frame dimensions")?;

        let out = cache_dir.join(format!("frame-{seq:04}.png"));
        img.save(&out).with_context(|| format!("cannot write {}", out.display()))?;
        paths.push(out);
    }

    paths.sort();
    Ok(paths)
}

// ── XMP + plist schedule parsing ──────────────────────────────────────────────

fn read_schedule(heic_path: &Path) -> Result<HeicSchedule> {
    use libheif_rs::HeifContext;

    let ctx = HeifContext::read_from_file(heic_path.to_str().context("non-UTF-8 path")?)?;
    let handle = ctx.primary_image_handle()?;

    // Find the first "mime" (XMP) metadata block.
    // Pre-allocate a buffer; 16 slots is more than enough for any HEIC file.
    let mut meta_buf = [0u32; 16];
    let n = handle.metadata_block_ids(&mut meta_buf, b"mime");
    if n == 0 {
        anyhow::bail!("no XMP metadata in HEIC");
    }
    let xmp_bytes = handle.metadata(meta_buf[0])?;

    parse_xmp_schedule(&xmp_bytes)
}

fn parse_xmp_schedule(xmp: &[u8]) -> Result<HeicSchedule> {
    // Walk the XMP XML and look for apple_desktop:{h24,solar,apr} attributes.
    let reader = xml::EventReader::new(xmp);
    for event in reader {
        let Ok(xml::reader::XmlEvent::StartElement { attributes, .. }) = event else {
            continue;
        };
        for attr in &attributes {
            let key = attr.name.local_name.as_str();
            if !matches!(key, "h24" | "solar" | "apr") {
                continue;
            }
            // Namespace check: should be apple_desktop or similar.
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(attr.value.trim())
                .context("base64 decode failed")?;

            match key {
                "h24" => return parse_h24_plist(&decoded),
                "solar" => {
                    // Solar support falls back to appearance or frame 0.
                    // Full solar requires location data (GeoClue2) — not yet implemented.
                    warn!("heic: solar schedule detected but not yet supported, using appearance fallback");
                    return parse_appearance_from_plist(&decoded);
                }
                "apr" => return parse_appearance_from_plist(&decoded),
                _ => {}
            }
        }
    }

    bail!("no apple_desktop schedule found in XMP")
}

// ── Plist structures ──────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct PlistH24 {
    #[serde(rename = "ti")]
    ti: Vec<PlistTimeItem>,
}

#[derive(serde::Deserialize)]
struct PlistTimeItem {
    #[serde(rename = "i")]
    i: usize,
    #[serde(rename = "t")]
    t: f64,
}

#[derive(serde::Deserialize)]
struct PlistAppearance {
    #[serde(rename = "l")]
    l: usize,
}

#[derive(serde::Deserialize)]
struct PlistApr {
    #[serde(rename = "ap")]
    ap: Option<PlistAppearance>,
}

fn parse_h24_plist(data: &[u8]) -> Result<HeicSchedule> {
    let parsed: PlistH24 = plist::from_bytes(data).context("H24 plist parse failed")?;
    let items = parsed
        .ti
        .into_iter()
        .map(|item| TimeItem { index: item.i, time: item.t })
        .collect();
    Ok(HeicSchedule::H24(items))
}

fn parse_appearance_from_plist(data: &[u8]) -> Result<HeicSchedule> {
    let parsed: PlistApr = plist::from_bytes(data).context("appearance plist parse failed")?;
    let ap = parsed.ap.unwrap_or(PlistAppearance { l: 0 });
    Ok(HeicSchedule::Appearance { light: ap.l })
}

// ── Frame selection ───────────────────────────────────────────────────────────

