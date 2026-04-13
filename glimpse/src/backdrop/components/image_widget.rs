use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::{Context, Result};
use relm4::gtk::{self, gdk, glib, prelude::*};
use relm4::prelude::*;

pub struct BackdropImageWidgetInit {
    pub path: PathBuf,
    pub width: i32,
    pub height: i32,
    pub blur_radius: u32,
}

pub struct BackdropImageWidget;

#[derive(Debug)]
pub enum BackdropImageWidgetMsg {
    Loaded(Result<DecodedBackdrop, String>),
}

#[derive(Debug)]
pub struct DecodedBackdrop {
    width: i32,
    height: i32,
    stride: usize,
    pixels: Vec<u8>,
}

#[relm4::component(pub)]
impl Component for BackdropImageWidget {
    type Init = BackdropImageWidgetInit;
    type Input = BackdropImageWidgetMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Picture {
            set_hexpand: true,
            set_vexpand: true,
            set_halign: gtk::Align::Fill,
            set_valign: gtk::Align::Fill,
            set_can_shrink: true,
            set_content_fit: gtk::ContentFit::Cover,
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        relm4::spawn({
            let sender = sender.input_sender().clone();
            let path = init.path.clone();
            let width = init.width;
            let height = init.height;
            let blur_radius = init.blur_radius;
            async move {
                let loaded = tokio::task::spawn_blocking(move || {
                    load_processed_image(&path, width, height, blur_radius)
                        .map(DecodedBackdrop::from)
                })
                .await
                .map_err(|error| format!("backdrop worker failed: {error}"))
                .and_then(|result| result.map_err(|error| error.to_string()));
                let _ = sender.send(BackdropImageWidgetMsg::Loaded(loaded));
            }
        });

        let model = BackdropImageWidget;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            BackdropImageWidgetMsg::Loaded(result) => match result {
                Ok(decoded) => root.set_paintable(Some(&decoded.into_texture())),
                Err(error) => tracing::warn!(%error, "backdrop: failed to load image"),
            },
        }
    }
}

impl DecodedBackdrop {
    fn into_texture(self) -> gdk::MemoryTexture {
        let bytes = glib::Bytes::from_owned(self.pixels);
        gdk::MemoryTexture::new(
            self.width,
            self.height,
            gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            self.stride,
        )
    }
}

impl From<image::RgbaImage> for DecodedBackdrop {
    fn from(image: image::RgbaImage) -> Self {
        let (width, height) = image.dimensions();
        Self {
            width: width as i32,
            height: height as i32,
            stride: (width * 4) as usize,
            pixels: image.into_raw(),
        }
    }
}

pub(crate) fn load_processed_image(
    path: &Path,
    width: i32,
    height: i32,
    blur_radius: u32,
) -> Result<image::RgbaImage> {
    load_processed_image_with_cache(path, width, height, blur_radius, cache_root().as_deref())
}

fn load_processed_image_with_cache(
    path: &Path,
    width: i32,
    height: i32,
    blur_radius: u32,
    cache_root: Option<&Path>,
) -> Result<image::RgbaImage> {
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
        if let Err(error) = write_cached_image(path, width, height, blur_radius, cache_root, &rgba)
        {
            tracing::warn!(%error, "backdrop: failed to update image cache");
        }
    }

    Ok(rgba)
}

fn resize_to_cover(image: image::DynamicImage, width: u32, height: u32) -> image::DynamicImage {
    image.resize_to_fill(width, height, image::imageops::FilterType::Lanczos3)
}

fn load_image(path: &Path) -> Result<image::DynamicImage> {
    if crate::wallpaper::heic::is_heic_path(path) {
        Ok(image::DynamicImage::ImageRgba8(
            crate::wallpaper::heic::decode(path)?.into_rgba_image(),
        ))
    } else {
        image::open(path).with_context(|| format!("failed to load {}", path.display()))
    }
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
) -> Result<Option<image::RgbaImage>> {
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
            tracing::warn!(path = %cache_path.display(), %error, "backdrop: failed to load cached image");
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
    image: &image::RgbaImage,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_processing_dimensions_downsample_large_blurs() {
        assert_eq!(blur_processing_dimensions(3840, 2160, 32), (960, 540, 8));
    }
}
