use anyhow::Context;
use image::{ImageBuffer, RgbaImage};
use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

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
    let image = lib.decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)?;

    let plane = image.planes().interleaved.context("no interleaved plane")?;
    let width = handle.width();
    let height = handle.height();
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
        assert!(is_heic_path(Path::new("/tmp/wallpaper.HEIF")));
        assert!(!is_heic_path(Path::new("/tmp/wallpaper.png")));
    }
}
