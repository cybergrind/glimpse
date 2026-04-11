use std::path::PathBuf;

use adw::prelude::*;
use relm4::gtk::{self, gdk, gio, glib};
use relm4::prelude::*;

use glimpse::wallpaper::ImageFit;

pub struct ImageWidgetInit {
    pub path: PathBuf,
    pub fit: ImageFit,
}

pub struct ImageWidget;

#[relm4::component(pub)]
impl SimpleComponent for ImageWidget {
    type Init = ImageWidgetInit;
    type Input = ();
    type Output = ();

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
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();
        set_wallpaper_image(&init.path, &root);
        let model = ImageWidget;
        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: (), _sender: ComponentSender<Self>) {}
}

pub fn set_wallpaper_image(path: &std::path::Path, picture: &gtk::Picture) {
    if !path.exists() {
        tracing::warn!(path = %path.display(), "wallpaper: file not found");
        return;
    }

    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Load synchronously so the picture has the correct natural size immediately —
    // set_file() is async and causes layout to be committed with natural size = 0,
    // which breaks content_fit scaling.
    let result: anyhow::Result<gdk::Paintable> = match ext.as_str() {
        "heic" | "heif" => crate::wallpaper::heic::load(path).map(|t| t.upcast()),
        "webp" => load_via_image_crate(path).map(|t| t.upcast()),
        _ => gdk::Texture::from_file(&gio::File::for_path(path))
            .map(|t| t.upcast())
            .map_err(|e| anyhow::anyhow!(e)),
    };

    match result {
        Ok(paintable) => picture.set_paintable(Some(&paintable)),
        Err(e) => tracing::warn!(path = %path.display(), "wallpaper: failed to load: {e}"),
    }
}


fn load_via_image_crate(path: &std::path::Path) -> anyhow::Result<gdk::MemoryTexture> {
    let img = image::open(path)?.into_rgba8();
    let (width, height) = img.dimensions();
    let bytes = glib::Bytes::from(img.as_raw().as_slice());
    Ok(gdk::MemoryTexture::new(
        width as i32,
        height as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        (width * 4) as usize,
    ))
}
