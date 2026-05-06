use anyhow::Context;
use image::{ImageBuffer, RgbaImage};
use libheif_rs::{ColorSpace, HeifContext, ImageHandle, LibHeif, RgbChroma};

pub struct DecodedHeic {
    pub width: i32,
    pub height: i32,
    pub stride: usize,
    pub pixels: Vec<u8>,
}

impl DecodedHeic {
    pub fn into_rgba_image(self) -> RgbaImage {
        ImageBuffer::from_raw(self.width as u32, self.height as u32, self.pixels)
            .expect("decoded HEIC dimensions should match pixel buffer")
    }
}

pub fn is_heic_path(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("heic" | "heif")
    )
}

pub fn decode(path: &std::path::Path) -> anyhow::Result<DecodedHeic> {
    let ctx = HeifContext::read_from_file(path.to_str().context("non-UTF-8 path")?)?;
    let lib = LibHeif::new();
    let handle = ctx.primary_image_handle()?;
    decode_handle(&lib, &handle)
}

pub fn decode_thumbnail(path: &std::path::Path) -> anyhow::Result<Option<DecodedHeic>> {
    let ctx = HeifContext::read_from_file(path.to_str().context("non-UTF-8 path")?)?;
    let lib = LibHeif::new();
    let handle = ctx.primary_image_handle()?;
    let thumbnail_count = handle.number_of_thumbnails();
    if thumbnail_count == 0 {
        return Ok(None);
    }

    let mut ids = vec![0; thumbnail_count];
    let actual_count = handle.thumbnail_ids(&mut ids);
    ids.truncate(actual_count);

    let mut best = None;
    for id in ids {
        let thumbnail = handle.thumbnail(id)?;
        let area = thumbnail.width() as u64 * thumbnail.height() as u64;
        if best
            .as_ref()
            .is_none_or(|(best_area, _): &(u64, ImageHandle)| area > *best_area)
        {
            best = Some((area, thumbnail));
        }
    }

    best.map(|(_, thumbnail)| decode_handle(&lib, &thumbnail))
        .transpose()
}

fn decode_handle(lib: &LibHeif, handle: &ImageHandle) -> anyhow::Result<DecodedHeic> {
    let image = lib.decode(handle, ColorSpace::Rgb(RgbChroma::Rgba), None)?;

    let plane = image.planes().interleaved.context("no interleaved plane")?;
    let width = image.width();
    let height = image.height();
    let row_bytes = (width * 4) as usize;
    let mut packed = Vec::with_capacity(row_bytes * height as usize);
    for row in 0..height as usize {
        let start = row * plane.stride;
        packed.extend_from_slice(&plane.data[start..start + row_bytes]);
    }

    Ok(DecodedHeic {
        width: width as i32,
        height: height as i32,
        stride: row_bytes,
        pixels: packed,
    })
}

#[cfg(test)]
mod tests {
    use super::is_heic_path;
    use std::path::Path;

    #[test]
    fn detects_heic_and_heif_paths_case_insensitively() {
        assert!(is_heic_path(Path::new("/tmp/wallpaper.heic")));
        assert!(is_heic_path(Path::new("/tmp/lock.HEIF")));
        assert!(!is_heic_path(Path::new("/tmp/wallpaper.png")));
    }
}
