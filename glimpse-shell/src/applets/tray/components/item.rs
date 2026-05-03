#![allow(unused_assignments)]

use std::path::{Path, PathBuf};

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gdk, gio, glib, prelude::*},
};

use crate::services::tray::{
    model::{Icon, Item, Status},
    protocol::ScrollOrientation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewModel {
    pub tooltip: Option<String>,
    pub status: Status,
    pub icon: Option<Icon>,
    pub overlay_icon: Option<Icon>,
    pub attention_icon: Option<Icon>,
    pub icon_theme_path: Option<String>,
}

impl From<&Item> for ViewModel {
    fn from(item: &Item) -> Self {
        Self {
            tooltip: tooltip_text(item),
            status: item.status,
            icon: item.icon.clone(),
            overlay_icon: item.overlay_icon.clone(),
            attention_icon: item.attention_icon.clone(),
            icon_theme_path: item.icon_theme_path.clone(),
        }
    }
}

pub struct TrayItem {
    view: ViewModel,
    icon_size: i32,
    main_image: gtk::Image,
    badge_image: gtk::Image,
}

#[derive(Debug)]
pub struct Init {
    pub view: ViewModel,
    pub icon_size: i32,
}

#[derive(Debug)]
pub enum Input {
    Update(ViewModel),
    SetIconSize(i32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    PrimaryClick {
        x: i32,
        y: i32,
    },
    MiddleClick {
        x: i32,
        y: i32,
    },
    ContextClick {
        x: i32,
        y: i32,
    },
    Scroll {
        delta: i32,
        orientation: ScrollOrientation,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for TrayItem {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        gtk::Button {
            add_css_class: "flat",
            add_css_class: "applet",
            #[watch]
            set_tooltip_text: model.view.tooltip.as_deref(),
            #[watch]
            set_has_tooltip: model.view.tooltip.is_some(),
            #[watch]
            set_css_classes: tray_item_classes(model.view.status),

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, x, y| {
                    let _ = sender.output(Output::PrimaryClick {
                        x: x as i32,
                        y: y as i32,
                    });
                }
            },

            add_controller = gtk::GestureClick {
                set_button: 3,
                connect_pressed[sender] => move |_, _, x, y| {
                    let _ = sender.output(Output::ContextClick {
                        x: x as i32,
                        y: y as i32,
                    });
                }
            },

            add_controller = gtk::GestureClick {
                set_button: 2,
                connect_pressed[sender] => move |_, _, x, y| {
                    let _ = sender.output(Output::MiddleClick {
                        x: x as i32,
                        y: y as i32,
                    });
                }
            },

            add_controller = gtk::EventControllerScroll {
                set_flags: gtk::EventControllerScrollFlags::BOTH_AXES,
                connect_scroll[sender] => move |_, dx, dy| {
                    if let Some((delta, orientation)) = scroll_event(dx, dy) {
                        let _ = sender.output(Output::Scroll { delta, orientation });
                    }
                    glib::Propagation::Proceed
                }
            },

            #[name(icon_overlay)]
            gtk::Overlay {
                #[name(main_image)]
                gtk::Image {
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
        let model = TrayItem {
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

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Update(view) => self.view = view,
            Input::SetIconSize(icon_size) => self.icon_size = icon_size,
        }
        self.apply_view();
    }
}

impl TrayItem {
    fn apply_view(&self) {
        self.apply_icon(
            &self.main_image,
            main_icon_for_view(&self.view),
            self.view.icon_theme_path.as_deref(),
            self.icon_size,
        );

        let badge_icon = badge_icon_for_view(&self.view);
        self.badge_image.set_visible(badge_icon.is_some());
        if let Some(icon) = badge_icon {
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
        icon: Option<&Icon>,
        icon_theme_path: Option<&str>,
        size: i32,
    ) {
        let icon_theme_path = icon_theme_path.filter(|path| !path.trim().is_empty());
        let direct_name_icon_path = match icon {
            Some(Icon::Name(name)) => direct_icon_path(name, icon_theme_path),
            _ => None,
        };

        image.set_pixel_size(size);
        image.set_size_request(size, size);
        image.clear();

        match icon {
            Some(Icon::Name(_)) if direct_name_icon_path.is_some() => {
                image.set_from_file(direct_name_icon_path);
                image.set_pixel_size(size);
            }
            Some(Icon::Name(name)) if icon_theme_path.is_none() => {
                image.set_icon_name(Some(name));
            }
            Some(Icon::Name(_)) => {
                match icon.and_then(|icon| icon_paintable(icon, icon_theme_path, size)) {
                    Some(paintable) => image.set_paintable(Some(&paintable)),
                    None => image.set_icon_name(Some("image-missing-symbolic")),
                }
            }
            Some(Icon::FilePath(path)) => {
                image.set_from_file(Some(path));
                image.set_pixel_size(size);
            }
            Some(Icon::Pixmap { .. }) | Some(Icon::EncodedBytes(_)) => {
                match icon.and_then(|icon| image_paintable(icon, icon_theme_path, size)) {
                    Some(paintable) => image.set_paintable(Some(&paintable)),
                    None => image.set_icon_name(Some("image-missing-symbolic")),
                }
            }
            None => image.set_icon_name(Some("image-missing-symbolic")),
        }
    }
}

pub fn icon_to_gicon(icon: &Icon) -> Option<gio::Icon> {
    match icon {
        Icon::Name(name) => Some(gio::ThemedIcon::new(name).upcast()),
        Icon::FilePath(path) => Some(gio::FileIcon::new(&gio::File::for_path(path)).upcast()),
        Icon::Pixmap {
            width,
            height,
            pixels,
        } => texture_from_pixmap(*width, *height, pixels).map(|texture| texture.upcast()),
        Icon::EncodedBytes(bytes) => {
            let bytes = glib::Bytes::from(bytes.as_slice());
            gdk::Texture::from_bytes(&bytes)
                .ok()
                .map(|texture| texture.upcast())
        }
    }
}

fn tray_item_classes(status: Status) -> &'static [&'static str] {
    match status {
        Status::NeedsAttention => &["flat", "applet", "needs-attention"],
        _ => &["flat", "applet"],
    }
}

fn scroll_event(dx: f64, dy: f64) -> Option<(i32, ScrollOrientation)> {
    let (amount, orientation) = if dy.abs() >= dx.abs() {
        (-dy, ScrollOrientation::Vertical)
    } else {
        (-dx, ScrollOrientation::Horizontal)
    };
    let delta = (amount * 120.0).round() as i32;
    (delta != 0).then_some((delta, orientation))
}

fn main_icon_for_view(view: &ViewModel) -> Option<&Icon> {
    view.icon.as_ref().or_else(|| {
        if view.status == Status::NeedsAttention {
            view.attention_icon.as_ref().or(view.overlay_icon.as_ref())
        } else {
            view.overlay_icon.as_ref()
        }
    })
}

fn badge_icon_for_view(view: &ViewModel) -> Option<&Icon> {
    if view.icon.is_none() {
        None
    } else if view.status == Status::NeedsAttention {
        view.attention_icon.as_ref().or(view.overlay_icon.as_ref())
    } else {
        view.overlay_icon.as_ref()
    }
}

fn tooltip_text(item: &Item) -> Option<String> {
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

    if item.status == Status::NeedsAttention {
        tooltip = Some(match tooltip {
            Some(text) if !text.is_empty() => format!("{text} - Needs attention"),
            _ => "Needs attention".into(),
        });
    }

    tooltip
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
    icon: &Icon,
    icon_theme_path: Option<&str>,
    size: i32,
) -> Option<gdk::Paintable> {
    match icon {
        Icon::Name(_) => icon_paintable(icon, icon_theme_path, size),
        Icon::FilePath(path) => {
            let file = gio::File::for_path(path);
            gdk::Texture::from_file(&file)
                .ok()
                .map(|texture| texture.upcast())
        }
        Icon::Pixmap {
            width,
            height,
            pixels,
        } => texture_from_pixmap(*width, *height, pixels).map(|texture| texture.upcast()),
        Icon::EncodedBytes(bytes) => {
            let bytes = glib::Bytes::from(bytes.as_slice());
            gdk::Texture::from_bytes(&bytes)
                .ok()
                .map(|texture| texture.upcast())
        }
    }
}

fn icon_paintable(icon: &Icon, icon_theme_path: Option<&str>, size: i32) -> Option<gdk::Paintable> {
    let Icon::Name(name) = icon else {
        return None;
    };

    let display = gdk::Display::default()?;
    let theme = gtk::IconTheme::for_display(&display);
    if let Some(theme_path) = icon_theme_path {
        let theme_path = Path::new(theme_path);
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
    use crate::services::tray::model::{Category, Tooltip};

    #[test]
    fn tooltip_prefers_description_then_title_then_fallback() {
        let item = test_item(Some(Tooltip {
            title: "Tooltip title".into(),
            description: "Tooltip description".into(),
            icon: None,
        }));

        assert_eq!(
            ViewModel::from(&item).tooltip.as_deref(),
            Some("Tooltip description")
        );
    }

    #[test]
    fn attention_status_prefers_attention_badge_and_tooltip_suffix() {
        let base_icon = Icon::Name("network-wireless-symbolic".into());
        let overlay_icon = Icon::Name("emblem-important-symbolic".into());
        let attention_icon = Icon::Name("dialog-warning-symbolic".into());
        let view = ViewModel {
            tooltip: Some("Downloads".into()),
            status: Status::NeedsAttention,
            icon: Some(base_icon.clone()),
            overlay_icon: Some(overlay_icon.clone()),
            attention_icon: Some(attention_icon.clone()),
            icon_theme_path: None,
        };

        assert_eq!(main_icon_for_view(&view), Some(&base_icon));
        assert_eq!(badge_icon_for_view(&view), Some(&attention_icon));
        assert_eq!(
            tooltip_text(&Item {
                status: Status::NeedsAttention,
                icon: Some(base_icon),
                overlay_icon: Some(overlay_icon),
                attention_icon: Some(attention_icon),
                tooltip: Some(Tooltip {
                    title: "Downloads".into(),
                    description: String::new(),
                    icon: None,
                }),
                ..test_item(None)
            })
            .as_deref(),
            Some("Downloads - Needs attention")
        );
    }

    #[test]
    fn scroll_event_prefers_larger_axis() {
        assert_eq!(
            scroll_event(0.0, -1.0),
            Some((120, ScrollOrientation::Vertical))
        );
        assert_eq!(
            scroll_event(2.0, 0.5),
            Some((-240, ScrollOrientation::Horizontal))
        );
    }

    #[test]
    fn pixmap_icons_create_paintables() {
        let icon = Icon::Pixmap {
            width: 1,
            height: 1,
            pixels: vec![255, 255, 255, 255],
        };

        assert!(image_paintable(&icon, None, 22).is_some());
    }

    fn test_item(tooltip: Option<Tooltip>) -> Item {
        Item {
            address: "org.example.App".into(),
            id: "example".into(),
            title: "Example".into(),
            status: Status::Active,
            category: Category::ApplicationStatus,
            item_is_menu: false,
            menu_path: String::new(),
            icon_theme_path: None,
            icon: None,
            overlay_icon: None,
            attention_icon: None,
            attention_movie_name: None,
            tooltip,
            menu: Vec::new(),
        }
    }
}
