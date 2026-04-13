#![allow(unused_assignments)]

use std::path::{Path, PathBuf};

use glimpse::tray::protocol::{TrayIcon, TrayItem, TrayStatus};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gdk, gio, glib, prelude::*},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayButtonView {
    pub tooltip: Option<String>,
    pub status: TrayStatus,
    pub icon: Option<TrayIcon>,
    pub overlay_icon: Option<TrayIcon>,
    pub attention_icon: Option<TrayIcon>,
    pub icon_theme_path: Option<String>,
}

impl From<&TrayItem> for TrayButtonView {
    fn from(item: &TrayItem) -> Self {
        Self {
            tooltip: button_tooltip(item),
            status: item.status,
            icon: item.icon.clone(),
            overlay_icon: item.overlay_icon.clone(),
            attention_icon: item.attention_icon.clone(),
            icon_theme_path: item.icon_theme_path.clone(),
        }
    }
}

pub struct TrayButton {
    view: TrayButtonView,
    icon_size: i32,
    main_image: gtk::Image,
    badge_image: gtk::Image,
}

#[derive(Debug)]
pub struct TrayButtonInit {
    pub view: TrayButtonView,
    pub icon_size: i32,
}

#[derive(Debug)]
pub enum TrayButtonInput {
    Update(TrayButtonView),
    SetIconSize(i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayButtonOutput {
    PrimaryClick { x: i32, y: i32 },
    SecondaryClick { x: i32, y: i32 },
}

#[relm4::component(pub)]
impl SimpleComponent for TrayButton {
    type Init = TrayButtonInit;
    type Input = TrayButtonInput;
    type Output = TrayButtonOutput;

    view! {
        gtk::Button {
            add_css_class: "flat",
            add_css_class: "tray-item",
            #[watch]
            set_tooltip_text: model.view.tooltip.as_deref(),

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, x, y| {
                    let _ = sender.output(TrayButtonOutput::PrimaryClick {
                        x: x as i32,
                        y: y as i32,
                    });
                }
            },

            add_controller = gtk::GestureClick {
                set_button: 3,
                connect_pressed[sender] => move |_, _, x, y| {
                    let _ = sender.output(TrayButtonOutput::SecondaryClick {
                        x: x as i32,
                        y: y as i32,
                    });
                }
            },

            #[name(icon_overlay)]
            gtk::Overlay {
                #[name(main_image)]
                gtk::Image {
                    set_pixel_size: model.icon_size,
                    set_valign: gtk::Align::Center,
                    set_halign: gtk::Align::Center,
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let sender = sender.clone();
        let model = TrayButton {
            view: init.view,
            icon_size: init.icon_size,
            main_image: gtk::Image::new(),
            badge_image: gtk::Image::new(),
        };
        let widgets = view_output!();

        let badge_image = gtk::Image::new();
        badge_image.set_visible(false);
        badge_image.set_valign(gtk::Align::End);
        badge_image.set_halign(gtk::Align::End);
        widgets.icon_overlay.add_overlay(&badge_image);
        widgets.icon_overlay.set_measure_overlay(&badge_image, true);

        let mut model = model;
        model.main_image = widgets.main_image.clone();
        model.badge_image = badge_image;
        model.apply_view();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            TrayButtonInput::Update(view) => self.view = view,
            TrayButtonInput::SetIconSize(icon_size) => self.icon_size = icon_size,
        }
        self.apply_view();
    }
}

impl TrayButton {
    fn apply_view(&self) {
        self.apply_icon(
            &self.main_image,
            main_icon_for_view(&self.view),
            self.view.icon_theme_path.as_deref(),
            self.icon_size,
        );

        let badge_visible = badge_icon_for_view(&self.view).is_some();
        self.badge_image.set_visible(badge_visible);
        if let Some(icon) = badge_icon_for_view(&self.view) {
            self.apply_icon(
                &self.badge_image,
                Some(icon),
                self.view.icon_theme_path.as_deref(),
                (self.icon_size / 2).max(8),
            );
        }
    }

    fn apply_icon(
        &self,
        image: &gtk::Image,
        icon: Option<&TrayIcon>,
        icon_theme_path: Option<&str>,
        size: i32,
    ) {
        let icon_theme_path = icon_theme_path.filter(|path| !path.trim().is_empty());
        let direct_name_icon_path = match icon {
            Some(TrayIcon::Name(name)) => direct_icon_path(name, icon_theme_path),
            _ => None,
        };
        image.set_pixel_size(size);
        image.set_size_request(size, size);
        image.clear();

        match icon {
            Some(TrayIcon::Name(_)) if direct_name_icon_path.is_some() => {
                image.set_from_file(direct_name_icon_path);
                image.set_pixel_size(size);
            }
            Some(TrayIcon::Name(name)) if icon_theme_path.is_none() => {
                image.set_icon_name(Some(name));
            }
            Some(TrayIcon::Name(_)) => {
                match icon_paintable(icon.expect("name icon present"), icon_theme_path, size) {
                    Some(paintable) => image.set_paintable(Some(&paintable)),
                    None => image.set_icon_name(Some("image-missing-symbolic")),
                }
            }
            Some(TrayIcon::FilePath(path)) => {
                image.set_from_file(Some(path));
                image.set_pixel_size(size);
            }
            Some(TrayIcon::Pixmap { .. }) | Some(TrayIcon::EncodedBytes(_)) => {
                match icon.and_then(|icon| image_paintable(icon, icon_theme_path, size)) {
                    Some(paintable) => image.set_paintable(Some(&paintable)),
                    None => image.set_icon_name(Some("image-missing-symbolic")),
                }
            }
            None => image.set_icon_name(Some("image-missing-symbolic")),
        }
    }
}

fn direct_icon_path(icon_name: &str, icon_theme_path: Option<&str>) -> Option<PathBuf> {
    let icon_theme_path = icon_theme_path.filter(|path| !path.trim().is_empty())?;
    if icon_name.trim().is_empty() {
        return None;
    }

    let base = Path::new(icon_theme_path);
    let name_path = Path::new(icon_name);

    if name_path.is_absolute() && name_path.is_file() {
        return Some(name_path.to_path_buf());
    }

    let candidates = if name_path.extension().is_some() {
        vec![base.join(name_path)]
    } else {
        vec![
            base.join(icon_name),
            base.join(format!("{icon_name}.png")),
            base.join(format!("{icon_name}.svg")),
            base.join(format!("{icon_name}.xpm")),
            base.join(format!("{icon_name}.ico")),
        ]
    };

    candidates.into_iter().find(|path| path.is_file())
}

fn image_paintable(
    icon: &TrayIcon,
    icon_theme_path: Option<&str>,
    size: i32,
) -> Option<gdk::Paintable> {
    match icon {
        TrayIcon::Name(_) => icon_paintable(icon, icon_theme_path, size),
        TrayIcon::FilePath(path) => {
            let file = gio::File::for_path(path);
            gdk::Texture::from_file(&file)
                .ok()
                .map(|texture| texture.upcast())
        }
        TrayIcon::Pixmap {
            width,
            height,
            pixels,
        } => texture_from_pixmap(*width, *height, pixels).map(|texture| texture.upcast()),
        TrayIcon::EncodedBytes(bytes) => {
            let bytes = glib::Bytes::from(bytes.as_slice());
            gdk::Texture::from_bytes(&bytes)
                .ok()
                .map(|texture| texture.upcast())
        }
    }
}

fn main_icon_for_view(view: &TrayButtonView) -> Option<&TrayIcon> {
    view.icon.as_ref().or_else(|| {
        if view.status == TrayStatus::NeedsAttention {
            view.attention_icon.as_ref().or(view.overlay_icon.as_ref())
        } else {
            view.overlay_icon.as_ref()
        }
    })
}

fn badge_icon_for_view(view: &TrayButtonView) -> Option<&TrayIcon> {
    if view.icon.is_none() {
        None
    } else if view.status == TrayStatus::NeedsAttention {
        view.attention_icon.as_ref().or(view.overlay_icon.as_ref())
    } else {
        view.overlay_icon.as_ref()
    }
}

fn button_tooltip(item: &TrayItem) -> Option<String> {
    let mut tooltip = if let Some(tooltip) = &item.tooltip {
        if !tooltip.description.is_empty() {
            Some(tooltip.description.clone())
        } else if !tooltip.title.is_empty() {
            Some(tooltip.title.clone())
        } else {
            None
        }
    } else {
        (!item.title.is_empty()).then(|| item.title.clone())
    };

    if item.status == TrayStatus::NeedsAttention {
        tooltip = Some(match tooltip {
            Some(text) if !text.is_empty() => format!("{text} - Needs attention"),
            _ => "Needs attention".into(),
        });
    }

    tooltip
}

fn icon_paintable(
    icon: &TrayIcon,
    icon_theme_path: Option<&str>,
    size: i32,
) -> Option<gdk::Paintable> {
    let TrayIcon::Name(name) = icon else {
        return None;
    };

    let display = gdk::Display::default()?;
    let theme = gtk::IconTheme::for_display(&display);
    if let Some(theme_path) = icon_theme_path {
        let theme_path = std::path::Path::new(theme_path);
        if !theme
            .search_path()
            .iter()
            .any(|existing| existing == theme_path)
        {
            theme.add_search_path(theme_path);
        }
    }

    Some(
        theme
            .lookup_icon(
                name,
                &[],
                size,
                1,
                gtk::TextDirection::None,
                gtk::IconLookupFlags::empty(),
            )
            .upcast(),
    )
}

pub fn icon_to_gicon(icon: &TrayIcon) -> Option<gio::Icon> {
    match icon {
        TrayIcon::Name(name) => Some(gio::ThemedIcon::new(name).upcast()),
        TrayIcon::FilePath(path) => Some(gio::FileIcon::new(&gio::File::for_path(path)).upcast()),
        TrayIcon::Pixmap {
            width,
            height,
            pixels,
        } => texture_from_pixmap(*width, *height, pixels).map(|texture| texture.upcast()),
        TrayIcon::EncodedBytes(bytes) => {
            let bytes = glib::Bytes::from(bytes.as_slice());
            gdk::Texture::from_bytes(&bytes)
                .ok()
                .map(|texture| texture.upcast())
        }
    }
}

fn texture_from_pixmap(width: i32, height: i32, pixels: &[u8]) -> Option<gdk::Texture> {
    if width <= 0 || height <= 0 {
        return None;
    }

    let stride = width as usize * 4;
    let expected = stride * height as usize;
    if pixels.len() < expected {
        return None;
    }

    let bytes = glib::Bytes::from_owned(pixels[..expected].to_vec());
    Some(
        gdk::MemoryTexture::new(width, height, gdk::MemoryFormat::A8r8g8b8, &bytes, stride)
            .upcast(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::tray::protocol::TrayTooltip;

    #[test]
    fn tooltip_prefers_description_then_title_then_fallback() {
        let item = TrayItem {
            address: "org.example.App".into(),
            id: "example".into(),
            title: "Visible title".into(),
            status: Default::default(),
            category: Default::default(),
            item_is_menu: false,
            menu_path: String::new(),
            icon_theme_path: None,
            icon: None,
            overlay_icon: None,
            attention_icon: None,
            attention_movie_name: None,
            tooltip: Some(TrayTooltip {
                title: "Tooltip title".into(),
                description: "Tooltip description".into(),
                icon: None,
            }),
            menu: Vec::new(),
        };

        assert_eq!(
            TrayButtonView::from(&item).tooltip.as_deref(),
            Some("Tooltip description")
        );
    }

    #[test]
    fn attention_status_prefers_attention_badge_and_tooltip_suffix() {
        let base_icon = TrayIcon::Name("network-wireless-symbolic".into());
        let overlay_icon = TrayIcon::Name("emblem-important-symbolic".into());
        let attention_icon = TrayIcon::Name("dialog-warning-symbolic".into());
        let view = TrayButtonView {
            tooltip: Some("Downloads".into()),
            status: TrayStatus::NeedsAttention,
            icon: Some(base_icon.clone()),
            overlay_icon: Some(overlay_icon.clone()),
            attention_icon: Some(attention_icon.clone()),
            icon_theme_path: None,
        };

        assert_eq!(
            button_tooltip(&TrayItem {
                address: "org.example.App".into(),
                id: "example".into(),
                title: "Downloads".into(),
                status: TrayStatus::NeedsAttention,
                category: Default::default(),
                item_is_menu: false,
                menu_path: String::new(),
                icon_theme_path: None,
                icon: Some(base_icon.clone()),
                overlay_icon: Some(overlay_icon.clone()),
                attention_icon: Some(attention_icon.clone()),
                attention_movie_name: None,
                tooltip: Some(TrayTooltip {
                    title: "Downloads".into(),
                    description: String::new(),
                    icon: None,
                }),
                menu: Vec::new(),
            })
            .as_deref(),
            Some("Downloads - Needs attention")
        );
        assert_eq!(main_icon_for_view(&view), Some(&base_icon));
        assert_eq!(badge_icon_for_view(&view), Some(&attention_icon));
    }

    #[test]
    fn direct_paintables_are_preferred_for_non_themed_icons() {
        let file_view = TrayButtonView {
            tooltip: None,
            status: TrayStatus::Active,
            icon: Some(TrayIcon::FilePath("/tmp/icon.png".into())),
            overlay_icon: None,
            attention_icon: None,
            icon_theme_path: None,
        };
        let pixmap_view = TrayButtonView {
            tooltip: None,
            status: TrayStatus::Active,
            icon: Some(TrayIcon::Pixmap {
                width: 1,
                height: 1,
                pixels: vec![255, 255, 255, 255],
            }),
            overlay_icon: None,
            attention_icon: None,
            icon_theme_path: None,
        };

        assert!(matches!(
            main_icon_for_view(&file_view),
            Some(TrayIcon::FilePath(_))
        ));
        assert!(matches!(
            main_icon_for_view(&pixmap_view),
            Some(TrayIcon::Pixmap { .. })
        ));
    }

    #[test]
    fn pixmap_icons_create_paintables() {
        let icon = TrayIcon::Pixmap {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };

        assert!(image_paintable(&icon, None, 22).is_some());
    }

    #[test]
    fn name_icons_prefer_native_image_icon_name_path() {
        let icon = TrayIcon::Name("image-missing-symbolic".into());

        assert!(icon_to_gicon(&icon).is_some());
    }

    #[test]
    fn theme_path_name_icons_resolve_direct_icon_files() {
        let temp_dir =
            std::env::temp_dir().join(format!("glimpse-tray-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let icon_path = temp_dir.join("steam_tray_mono.png");
        std::fs::write(&icon_path, TEST_PNG_BYTES).expect("should write test icon");

        let resolved =
            direct_icon_path("steam_tray_mono", Some(temp_dir.to_string_lossy().as_ref()));

        assert_eq!(resolved.as_deref(), Some(icon_path.as_path()));

        let _ = std::fs::remove_file(&icon_path);
        let _ = std::fs::remove_dir(&temp_dir);
    }

    const TEST_PNG_BYTES: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D',
        b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00,
        0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
    ];
}
