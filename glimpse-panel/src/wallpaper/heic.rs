use anyhow::Context;
use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};
use relm4::gtk::{gdk, glib};

pub fn load(path: &std::path::Path) -> anyhow::Result<gdk::MemoryTexture> {
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

    let bytes = glib::Bytes::from(&packed);
    Ok(gdk::MemoryTexture::new(
        w as i32,
        h as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        row_bytes,
    ))
}
