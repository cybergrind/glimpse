use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result, anyhow};
use css_color::Srgb;
use gtk::{
    Align,
    gdk::{self, MemoryFormat},
    glib,
};
use gtk4 as gtk;
use gtk4::prelude::{Cast, DrawingAreaExtManual, WidgetExt};
use image::{DynamicImage, RgbaImage, imageops::FilterType};
use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

use super::{BackdropConfig, BackdropMode};

pub fn build_backdrop_widget(
    config: &BackdropConfig,
    width: i32,
    height: i32,
) -> Result<gtk::Widget> {
    match config.mode {
        BackdropMode::Color => Ok(build_color_widget(&config.color)?.upcast()),
        BackdropMode::Image => {
            let path = config
                .path
                .as_deref()
                .context("backdrop image mode requires 'path'")?;
            if !path.is_file() {
                return Err(anyhow!("failed to load {}", path.display()));
            }

            Ok(build_image_widget(path.to_path_buf(), width, height, config.blur_radius).upcast())
        }
    }
}

fn build_image_widget(path: PathBuf, width: i32, height: i32, blur_radius: u32) -> gtk::Picture {
    let picture = gtk::Picture::new();
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.set_halign(Align::Fill);
    picture.set_valign(Align::Fill);
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Cover);

    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = load_processed_image(&path, width, height, blur_radius)
            .map(ImageTextureData::from)
            .map_err(|error| error.to_string());
        let _ = tx.send(result);
    });

    let picture_clone = picture.clone();
    glib::timeout_add_local(Duration::from_millis(16), move || match rx.try_recv() {
        Ok(Ok(image)) => {
            let texture = memory_texture_from_data(image);
            picture_clone.set_paintable(Some(&texture));
            glib::ControlFlow::Break
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, "backdrop: failed to load image");
            glib::ControlFlow::Break
        }
        Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
        Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
    });

    picture
}

fn build_color_widget(color: &str) -> Result<gtk::DrawingArea> {
    let parsed = color
        .parse::<Srgb>()
        .map_err(|_| anyhow!("invalid backdrop color '{color}'"))?;

    let drawing = gtk::DrawingArea::new();
    drawing.set_hexpand(true);
    drawing.set_vexpand(true);
    drawing.set_draw_func(move |_, cr, _, _| {
        cr.set_source_rgba(
            parsed.red as f64,
            parsed.green as f64,
            parsed.blue as f64,
            parsed.alpha as f64,
        );
        let _ = cr.paint();
    });
    Ok(drawing)
}

pub(crate) fn load_processed_image(
    path: &Path,
    width: i32,
    height: i32,
    blur_radius: u32,
) -> Result<RgbaImage> {
    load_processed_image_with_cache(path, width, height, blur_radius, cache_root().as_deref())
}

fn load_processed_image_with_cache(
    path: &Path,
    width: i32,
    height: i32,
    blur_radius: u32,
    cache_root: Option<&Path>,
) -> Result<RgbaImage> {
    let width = width.max(1) as u32;
    let height = height.max(1) as u32;

    if let Some(cached) = maybe_load_cached_image(path, width, height, blur_radius, cache_root)? {
        return Ok(cached);
    }

    let image = load_image(path)?;
    let processed = if blur_radius > 0 {
        let (work_width, work_height, work_blur_radius) =
            blur_processing_dimensions(width, height, blur_radius);
        let resized = resize_to_cover(image, work_width, work_height);
        let blurred = resized.blur(work_blur_radius as f32);
        resize_to_cover(blurred, width, height)
    } else {
        resize_to_cover(image, width, height)
    };

    let rgba = processed.to_rgba8();

    if let Some(cache_root) = cache_root {
        if let Err(error) =
            write_cached_image(path, width, height, blur_radius, cache_root, &rgba)
        {
            tracing::warn!(%error, "backdrop: failed to update image cache");
        }
    }

    Ok(rgba)
}

fn resize_to_cover(image: DynamicImage, width: u32, height: u32) -> DynamicImage {
    image.resize_to_fill(width, height, FilterType::Lanczos3)
}

fn load_image(path: &Path) -> Result<DynamicImage> {
    if is_heic_path(path) {
        load_heic_image(path)
    } else {
        image::open(path).with_context(|| format!("failed to load {}", path.display()))
    }
}

fn load_heic_image(path: &Path) -> Result<DynamicImage> {
    let ctx = HeifContext::read_from_file(path.to_str().context("non-UTF-8 path")?)
        .with_context(|| format!("failed to load {}", path.display()))?;
    let lib = LibHeif::new();
    let handle = ctx
        .primary_image_handle()
        .with_context(|| format!("failed to load {}", path.display()))?;
    let image = lib
        .decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)
        .with_context(|| format!("failed to load {}", path.display()))?;

    let plane = image
        .planes()
        .interleaved
        .context("no interleaved plane")?;
    let width = handle.width();
    let height = handle.height();
    let row_bytes = (width * 4) as usize;
    let mut packed = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = row * plane.stride;
        packed.extend_from_slice(&plane.data[start..start + row_bytes]);
    }

    let rgba = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(width, height, packed)
        .context("invalid HEIC frame dimensions")?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

pub(crate) fn is_heic_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("heic" | "heif")
    )
}

fn blur_processing_dimensions(width: u32, height: u32, blur_radius: u32) -> (u32, u32, u32) {
    let divisor = (blur_radius / 8).clamp(1, 4);
    let work_width = (width / divisor).max(1);
    let work_height = (height / divisor).max(1);
    let work_blur_radius = (blur_radius / divisor).max(1);
    (work_width, work_height, work_blur_radius)
}

fn cache_root() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("glimpse").join("backdrop"))
}

fn processed_cache_path(
    source_path: &Path,
    width: u32,
    height: u32,
    blur_radius: u32,
    cache_root: &Path,
) -> Result<PathBuf> {
    let mut hasher = DefaultHasher::new();
    source_path.hash(&mut hasher);
    width.hash(&mut hasher);
    height.hash(&mut hasher);
    blur_radius.hash(&mut hasher);
    let digest = hasher.finish();

    Ok(cache_root.join(format!("{digest:016x}.png")))
}

fn cache_metadata_path(cache_path: &Path) -> PathBuf {
    cache_path.with_extension("meta")
}

fn maybe_load_cached_image(
    source_path: &Path,
    width: u32,
    height: u32,
    blur_radius: u32,
    cache_root: Option<&Path>,
) -> Result<Option<RgbaImage>> {
    let Some(cache_root) = cache_root else {
        return Ok(None);
    };

    let cache_path = processed_cache_path(source_path, width, height, blur_radius, cache_root)?;
    if !cache_path.is_file() {
        return Ok(None);
    }

    let metadata_path = cache_metadata_path(&cache_path);
    if !metadata_path.is_file() {
        return Ok(None);
    }

    if !cached_image_matches_source(source_path, &metadata_path)? {
        return Ok(None);
    }

    match image::open(&cache_path) {
        Ok(cached) => Ok(Some(cached.to_rgba8())),
        Err(error) => {
            tracing::warn!(
                path = %cache_path.display(),
                %error,
                "backdrop: failed to load cached image"
            );
            Ok(None)
        }
    }
}

fn cached_image_matches_source(source_path: &Path, metadata_path: &Path) -> Result<bool> {
    let cached_signature = fs::read_to_string(metadata_path)
        .with_context(|| format!("failed to read {}", metadata_path.display()))?;

    match source_signature(source_path)? {
        Some(current_signature) => Ok(current_signature == cached_signature),
        None => Ok(true),
    }
}

fn write_cached_image(
    source_path: &Path,
    width: u32,
    height: u32,
    blur_radius: u32,
    cache_root: &Path,
    image: &RgbaImage,
) -> Result<()> {
    let cache_path = processed_cache_path(source_path, width, height, blur_radius, cache_root)?;
    let metadata_path = cache_metadata_path(&cache_path);

    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    image
        .save(&cache_path)
        .with_context(|| format!("failed to write {}", cache_path.display()))?;

    if let Some(signature) = source_signature(source_path)? {
        fs::write(&metadata_path, signature)
            .with_context(|| format!("failed to write {}", metadata_path.display()))?;
    }

    Ok(())
}

fn source_signature(source_path: &Path) -> Result<Option<String>> {
    let metadata = match fs::metadata(source_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", source_path.display()));
        }
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| (duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or((0, 0));

    Ok(Some(format!(
        "{}:{}:{}",
        metadata.len(),
        modified.0,
        modified.1
    )))
}

fn memory_texture_from_data(image: ImageTextureData) -> gdk::MemoryTexture {
    let bytes = glib::Bytes::from_owned(image.bytes);

    gdk::MemoryTexture::new(
        image.width,
        image.height,
        MemoryFormat::R8g8b8a8,
        &bytes,
        image.stride,
    )
}

struct ImageTextureData {
    width: i32,
    height: i32,
    stride: usize,
    bytes: Vec<u8>,
}

impl From<RgbaImage> for ImageTextureData {
    fn from(image: RgbaImage) -> Self {
        let (width, height) = image.dimensions();
        Self {
            width: width as i32,
            height: height as i32,
            stride: (width * 4) as usize,
            bytes: image.into_raw(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        blur_processing_dimensions, is_heic_path, load_processed_image, load_processed_image_with_cache,
        processed_cache_path,
    };
    use image::{ImageBuffer, Rgba};
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_png_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos();
        std::env::temp_dir().join(format!("glimpse-backdrop-test-{nanos}.png"))
    }

    fn write_test_png(path: &PathBuf, width: u32, height: u32) {
        let image = ImageBuffer::from_fn(width, height, |x, y| {
            Rgba([x as u8, y as u8, 128, 255])
        });
        image.save(path).expect("save test image");
    }

    #[test]
    fn resized_image_matches_requested_output_size() {
        let path = temp_png_path();
        write_test_png(&path, 40, 20);

        let image = load_processed_image(&path, 24, 24, 0).expect("processed image");
        fs::remove_file(&path).ok();

        assert_eq!(image.width(), 24);
        assert_eq!(image.height(), 24);
    }

    #[test]
    fn missing_image_returns_error() {
        let path = temp_png_path();
        let error = load_processed_image(&path, 32, 32, 8).expect_err("missing image should fail");
        assert!(
            error.to_string().contains("failed to load")
                || error.to_string().contains("No such file")
        );
    }

    #[test]
    fn detects_heic_and_heif_paths_case_insensitively() {
        assert!(is_heic_path(Path::new("/tmp/backdrop.heic")));
        assert!(is_heic_path(Path::new("/tmp/backdrop.HEIF")));
        assert!(!is_heic_path(Path::new("/tmp/backdrop.png")));
    }

    #[test]
    fn blur_processing_reduces_large_workload() {
        let (width, height, radius) = blur_processing_dimensions(3072, 1728, 24);

        assert_eq!((width, height, radius), (1024, 576, 8));
    }

    #[test]
    fn blur_processing_keeps_small_sizes_non_zero() {
        let (width, height, radius) = blur_processing_dimensions(3, 2, 24);

        assert_eq!((width, height, radius), (1, 1, 8));
    }

    #[test]
    fn cache_path_changes_with_blur_radius() {
        let path = temp_png_path();
        let cache_dir = std::env::temp_dir().join("glimpse-backdrop-cache-test-a");
        write_test_png(&path, 8, 8);

        let a = processed_cache_path(&path, 24, 24, 8, &cache_dir).expect("cache path");
        let b = processed_cache_path(&path, 24, 24, 24, &cache_dir).expect("cache path");

        fs::remove_file(&path).ok();

        assert_ne!(a, b);
    }

    #[test]
    fn processed_image_can_be_loaded_from_cache_without_source() {
        let path = temp_png_path();
        let cache_dir = std::env::temp_dir().join(format!(
            "glimpse-backdrop-cache-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time went backwards")
                .as_nanos()
        ));
        write_test_png(&path, 40, 20);

        let first = load_processed_image_with_cache(&path, 24, 24, 12, Some(&cache_dir))
            .expect("first processed image");
        fs::remove_file(&path).ok();
        let second = load_processed_image_with_cache(&path, 24, 24, 12, Some(&cache_dir))
            .expect("cached processed image");

        fs::remove_dir_all(&cache_dir).ok();

        assert_eq!(first.dimensions(), second.dimensions());
    }
}
