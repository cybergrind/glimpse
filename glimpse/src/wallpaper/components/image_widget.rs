use std::path::PathBuf;

use adw::prelude::*;
use relm4::gtk::{self, gdk, glib};
use relm4::prelude::*;

use crate::wallpaper::ImageFit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageWidgetInit {
    pub path: PathBuf,
    pub fit: ImageFit,
}

pub struct ImageWidget {
    request_id: u64,
    current: ImageWidgetInit,
}

#[derive(Debug)]
pub enum ImageWidgetMsg {
    Reconfigure(ImageWidgetInit),
    Loaded {
        request_id: u64,
        result: Result<DecodedWallpaper, String>,
    },
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
    type Input = ImageWidgetMsg;
    type Output = ();
    type CommandOutput = ();

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

    fn init(init: Self::Init, _root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let widgets = view_output!();
        let model = ImageWidget {
            request_id: 0,
            current: init,
        };
        sender.input(ImageWidgetMsg::Reconfigure(model.current.clone()));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            ImageWidgetMsg::Reconfigure(next) => {
                let fit_changed = self.current.fit != next.fit;
                let path_changed = self.current.path != next.path;
                self.current = next.clone();
                if fit_changed {
                    root.set_content_fit(next.fit.to_gtk());
                }
                if path_changed {
                    root.set_paintable(None::<&gdk::Paintable>);
                }
                if path_changed || self.request_id == 0 {
                    self.request_id += 1;
                    spawn_wallpaper_load(self.request_id, next.path, sender.input_sender().clone());
                }
            }
            ImageWidgetMsg::Loaded { request_id, result } => {
                if request_id != self.request_id {
                    return;
                }

                match result {
                    Ok(decoded) => root.set_paintable(Some(&decoded.into_texture())),
                    Err(error) => tracing::warn!("wallpaper: failed to load: {error}"),
                }
            }
        }
    }
}

fn spawn_wallpaper_load(
    request_id: u64,
    path: PathBuf,
    sender: relm4::Sender<ImageWidgetMsg>,
) {
    relm4::spawn(async move {
        tracing::info!("loading wallpaper image");
        let result = tokio::task::spawn_blocking(move || decode_wallpaper(&path))
            .await
            .map_err(|error| format!("wallpaper worker failed: {error}"))
            .and_then(|result| result.map_err(|error| error.to_string()));
        match &result {
            Ok(_) => tracing::info!("image decoded and loaded"),
            Err(error) => tracing::warn!("wallpaper: failed to decode image: {error}"),
        }
        let _ = sender.send(ImageWidgetMsg::Loaded { request_id, result });
    });
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
