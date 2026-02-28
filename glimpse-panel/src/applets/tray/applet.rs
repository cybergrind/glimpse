use std::collections::HashMap;

use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, gdk, glib, prelude::*},
};
use system_tray::{
    client::{Client, Event, UpdateEvent},
    item::{IconPixmap, StatusNotifierItem},
};

use crate::applets::tray::TrayConfig;

pub struct Tray {
    config: TrayConfig,
    items: HashMap<String, gtk::Button>,
}

pub struct TrayInit {
    pub config: TrayConfig,
}

#[derive(Debug)]
pub enum TrayInput {
    ItemAdded(String, Box<StatusNotifierItem>),
    ItemUpdated(String, UpdateEvent),
    ItemRemoved(String),
}

#[derive(Debug)]
pub enum TrayCommand {
    ItemAdded(String, Box<StatusNotifierItem>),
    ItemUpdated(String, UpdateEvent),
    ItemRemoved(String),
}

#[relm4::component(pub)]
impl Component for Tray {
    type Init = TrayInit;
    type Input = TrayInput;
    type Output = ();
    type CommandOutput = TrayCommand;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "applet",
            add_css_class: "tray",
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Tray {
            config: init.config,
            items: HashMap::new(),
        };
        let widgets = view_output!();

        sender.command(|out, shutdown| {
            shutdown
                .register(async move {
                    let client = match Client::new().await {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!("failed to create tray client: {e}");
                            return;
                        }
                    };

                    let mut rx = client.subscribe();
                    loop {
                        let Ok(event) = rx.recv().await else { break };
                        let cmd = match event {
                            Event::Add(address, item) => TrayCommand::ItemAdded(address, item),
                            Event::Update(address, event) => {
                                TrayCommand::ItemUpdated(address, event)
                            }
                            Event::Remove(address) => TrayCommand::ItemRemoved(address),
                        };
                        out.send(cmd).ok();
                    }
                })
                .drop_on_shutdown()
        });

        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            TrayCommand::ItemAdded(address, item) => {
                sender.input(TrayInput::ItemAdded(address, item))
            }
            TrayCommand::ItemUpdated(address, item) => {
                sender.input(TrayInput::ItemUpdated(address, item))
            }
            TrayCommand::ItemRemoved(address) => sender.input(TrayInput::ItemRemoved(address)),
        }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            TrayInput::ItemAdded(address, item) => {
                let btn = make_icon_button(&item, self.config.icon_size);
                root.append(&btn);
                self.items.insert(address, btn);
            }
            TrayInput::ItemUpdated(address, event) => {
                if let UpdateEvent::Icon {
                    icon_name,
                    icon_pixmap,
                } = event
                {
                    if let Some(btn) = self.items.get(&address) {
                        if let Some(image) = btn.child().and_downcast::<gtk::Image>() {
                            update_icon(&image, icon_name.as_deref(), icon_pixmap.as_deref(), self.config.icon_size);
                        }
                    }
                }
            }
            TrayInput::ItemRemoved(address) => {
                if let Some(btn) = self.items.remove(&address) {
                    root.remove(&btn);
                }
            }
        }
    }
}
fn make_icon_button(item: &StatusNotifierItem, size: i32) -> gtk::Button {
    let image = gtk::Image::new();
    image.set_pixel_size(size);
    update_icon(
        &image,
        item.icon_name.as_deref(),
        item.icon_pixmap.as_deref(),
        size,
    );

    let btn = gtk::Button::new();
    btn.set_child(Some(&image));
    btn.add_css_class("flat");
    btn.add_css_class("tray-item");
    btn
}

fn update_icon(
    image: &gtk::Image,
    icon_name: Option<&str>,
    icon_pixmap: Option<&[IconPixmap]>,
    size: i32,
) {
    if let Some(name) = icon_name.filter(|n| !n.is_empty()) {
        image.set_icon_name(Some(name));
        return;
    }

    if let Some(pixmaps) = icon_pixmap {
        if let Some(pixmap) = pixmaps.iter().max_by_key(|p| p.width) {
            let width = pixmap.width as usize;
            let height = pixmap.height as usize;
            let argb = &pixmap.pixels;

            // ARGB32 network byte order → BGRA (GDK MemoryFormat::B8g8r8a8)
            let mut bgra = vec![0u8; argb.len()];
            for i in (0..argb.len()).step_by(4) {
                bgra[i] = argb[i + 3]; // B
                bgra[i + 1] = argb[i + 2]; // G
                bgra[i + 2] = argb[i + 1]; // R
                bgra[i + 3] = argb[i]; // A
            }

            let bytes = glib::Bytes::from_owned(bgra);
            let texture = gdk::MemoryTexture::new(
                width as i32,
                height as i32,
                gdk::MemoryFormat::B8g8r8a8,
                &bytes,
                width * 4,
            );
            image.set_paintable(Some(&texture));
            image.set_pixel_size(size);
            return;
        }
    }

    image.set_icon_name(Some("image-missing-symbolic"));
}
