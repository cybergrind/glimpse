use std::path::PathBuf;

use adw::prelude::*;
use relm4::gtk::{self, gdk, glib};
use relm4::prelude::*;

use glimpse::wallpaper::ImageFit;

pub struct ImageWidgetInit {
    pub path: PathBuf,
    pub fit: ImageFit,
}

pub struct ImageWidget;

#[derive(Debug)]
pub enum ImageWidgetMsg {
    Loaded(Result<DecodedWallpaper, String>),
}

#[derive(Debug)]
pub struct DecodedWallpaper {
    width: i32,
    height: i32,
    stride: usize,
    pixels: Vec<u8>,
}

#[relm4::component(pub)]
impl Component for ImageWidget {
    type Init = ImageWidgetInit;
    type Input = ();
    type Output = ();
    type CommandOutput = ImageWidgetMsg;

    view! {
        gtk::Picture {
            set_hexpand: true,
            set_vexpand: true,
            set_halign: gtk::Align::Fill,
            set_valign: gtk::Align::Fill,
            set_can_shrink: true,
            set_content_fit: init.fit.to_gtk(),
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        let model = ImageWidget;

        sender.command(move |out, shutdown| {
            let path = init.path.clone();
            shutdown
                .register(async move {
                    let loaded = tokio::task::spawn_blocking(move || decode_wallpaper(&path))
                        .await
                        .map_err(|error| format!("wallpaper worker failed: {error}"))
                        .and_then(|result| result.map_err(|error| error.to_string()));
                    let _ = out.send(ImageWidgetMsg::Loaded(loaded));
                })
                .drop_on_shutdown()
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: (), _sender: ComponentSender<Self>, _root: &Self::Root) {}

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        let ImageWidgetMsg::Loaded(result) = msg;
        match result {
            Ok(decoded) => root.set_paintable(Some(&decoded.into_texture())),
            Err(error) => tracing::warn!("wallpaper: failed to load: {error}"),
        }
    }
}

impl DecodedWallpaper {
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

fn decode_wallpaper(path: &std::path::Path) -> anyhow::Result<DecodedWallpaper> {
    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }

    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "heic" | "heif" => crate::wallpaper::heic::decode(path).map(|decoded| DecodedWallpaper {
            width: decoded.width,
            height: decoded.height,
            stride: decoded.stride,
            pixels: decoded.pixels,
        }),
        _ => decode_via_image_crate(path),
    }
}

fn decode_via_image_crate(path: &std::path::Path) -> anyhow::Result<DecodedWallpaper> {
    let img = image::open(path)?.into_rgba8();
    let (width, height) = img.dimensions();
    Ok(DecodedWallpaper {
        width: width as i32,
        height: height as i32,
        stride: (width * 4) as usize,
        pixels: img.into_raw(),
    })
}
