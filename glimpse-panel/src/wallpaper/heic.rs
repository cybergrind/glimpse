use anyhow::Context;
use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

pub struct DecodedHeic {
    pub width: i32,
    pub height: i32,
    pub stride: usize,
    pub pixels: Vec<u8>,
}

pub fn decode(path: &std::path::Path) -> anyhow::Result<DecodedHeic> {
    let ctx = HeifContext::read_from_file(path.to_str().context("non-UTF-8 path")?)?;
    let lib = LibHeif::new();
    let handle = ctx.primary_image_handle()?;
    let image = lib.decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)?;

    let plane = image.planes().interleaved.context("no interleaved plane")?;
    let w = handle.width();
    let h = handle.height();
    let row_bytes = (w * 4) as usize;
    let mut packed = Vec::with_capacity(row_bytes * h as usize);
    for row in 0..h as usize {
        let start = row * plane.stride;
        packed.extend_from_slice(&plane.data[start..start + row_bytes]);
    }

    Ok(DecodedHeic {
        width: w as i32,
        height: h as i32,
        stride: row_bytes,
        pixels: packed,
    })
}
