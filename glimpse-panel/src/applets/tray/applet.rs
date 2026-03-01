use std::collections::HashMap;

use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, gdk, gio, glib, prelude::*},
};
use system_tray::{
    client::{ActivateRequest, Client, Event, UpdateEvent},
    item::{IconPixmap, StatusNotifierItem},
    menu::{MenuItem, MenuType, TrayMenu},
};
use tokio::sync::mpsc;

use crate::applets::tray::TrayConfig;

struct TrayItem {
    button: gtk::Button,
    menu: Option<TrayMenu>,
    menu_path: Option<String>,
    popover: Option<gtk::PopoverMenu>,
}

pub struct Tray {
    config: TrayConfig,
    items: HashMap<String, TrayItem>,
    activate_tx: mpsc::Sender<ActivateRequest>,
}

pub struct TrayInit {
    pub config: TrayConfig,
}

#[derive(Debug)]
pub enum TrayInput {
    ItemAdded(String, Box<StatusNotifierItem>),
    ItemUpdated(String, UpdateEvent),
    ItemRemoved(String),
    ShowMenu(String),
    ActivateDefault(String),
    ActivateItem { address: String, submenu_id: i32 },
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
        let (activate_tx, mut activate_rx) = mpsc::channel::<ActivateRequest>(8);

        let model = Tray {
            config: init.config,
            items: HashMap::new(),
            activate_tx,
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
                        tokio::select! {
                            Ok(event) = rx.recv() => {
                                let cmd = match event {
                                    Event::Add(address, item) => TrayCommand::ItemAdded(address, item),
                                    Event::Update(address, event) => TrayCommand::ItemUpdated(address, event),
                                    Event::Remove(address) => TrayCommand::ItemRemoved(address),
                                };
                                out.send(cmd).ok();
                            }
                            Some(req) = activate_rx.recv() => {
                                if let Err(e) = client.activate(req).await {
                                    tracing::error!("activate failed: {e}");
                                }
                            }
                        }
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

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            TrayInput::ItemAdded(address, item) => {
                let btn = make_icon_button(&item, self.config.icon_size);

                let left = gtk::GestureClick::new();
                left.set_button(1);
                let left_sender = sender.clone();
                let left_address = address.clone();
                left.connect_pressed(move |_, _, _, _| {
                    left_sender.input(TrayInput::ActivateDefault(left_address.clone()));
                });
                btn.add_controller(left);

                let right = gtk::GestureClick::new();
                right.set_button(3);
                let right_sender = sender.clone();
                let right_address = address.clone();
                right.connect_pressed(move |_, _, _, _| {
                    right_sender.input(TrayInput::ShowMenu(right_address.clone()));
                });
                btn.add_controller(right);

                root.append(&btn);
                self.items.insert(address, TrayItem {
                    button: btn,
                    menu: None,
                    menu_path: None,
                    popover: None,
                });
            }
            TrayInput::ItemUpdated(address, event) => match event {
                UpdateEvent::Icon { icon_name, icon_pixmap } => {
                    if let Some(tray_item) = self.items.get(&address) {
                        if let Some(image) = tray_item.button.child().and_downcast::<gtk::Image>() {
                            update_icon(
                                &image,
                                icon_name.as_deref(),
                                icon_pixmap.as_deref(),
                                self.config.icon_size,
                            );
                        }
                    }
                }
                UpdateEvent::Menu(menu) => {
                    if let Some(tray_item) = self.items.get_mut(&address) {
                        let gio_menu = build_gio_menu(&menu.submenus);
                        let group = gio::SimpleActionGroup::new();
                        register_actions(&menu.submenus, &address, &group, &sender);
                        tray_item.button.insert_action_group("tray", Some(&group));
                        let popover = gtk::PopoverMenu::from_model(Some(&gio_menu));
                        popover.set_parent(&tray_item.button);
                        tray_item.popover = Some(popover);
                        tray_item.menu = Some(menu);
                    }
                }
                UpdateEvent::MenuConnect(path) => {
                    if let Some(tray_item) = self.items.get_mut(&address) {
                        tray_item.menu_path = Some(path);
                    }
                }
                _ => {}
            },
            TrayInput::ItemRemoved(address) => {
                if let Some(tray_item) = self.items.remove(&address) {
                    root.remove(&tray_item.button);
                }
            }
            TrayInput::ShowMenu(address) => {
                if let Some(tray_item) = self.items.get(&address) {
                    if let Some(popover) = &tray_item.popover {
                        popover.popup();
                    }
                }
            }
            TrayInput::ActivateDefault(address) => {
                self.activate_tx
                    .try_send(ActivateRequest::Default { address, x: 0, y: 0 })
                    .ok();
            }
            TrayInput::ActivateItem { address, submenu_id } => {
                let menu_path = self.items.get(&address)
                    .and_then(|i| i.menu_path.clone())
                    .unwrap_or_default();
                self.activate_tx
                    .try_send(ActivateRequest::MenuItem { address, menu_path, submenu_id })
                    .ok();
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

fn build_gio_menu(items: &[MenuItem]) -> gio::Menu {
    let menu = gio::Menu::new();
    let mut section_items = gio::Menu::new();

    for item in items {
        if !item.visible {
            continue;
        }
        match item.menu_type {
            MenuType::Separator => {
                menu.append_section(None, &section_items);
                section_items = gio::Menu::new();
            }
            MenuType::Standard => {
                let label = clean_label(item.label.as_deref().unwrap_or(""));
                if !item.submenu.is_empty() {
                    let submenu = build_gio_menu(&item.submenu);
                    section_items.append_submenu(Some(&label), &submenu);
                } else {
                    let action = format!("tray.item-{}", item.id);
                    section_items.append(Some(&label), Some(&action));
                }
            }
        }
    }
    menu.append_section(None, &section_items);
    menu
}

fn clean_label(s: &str) -> String {
    s.replace("__", "\x00")
        .replace('_', "")
        .replace('\x00', "_")
}

fn register_actions(
    items: &[MenuItem],
    address: &str,
    group: &gio::SimpleActionGroup,
    sender: &ComponentSender<Tray>,
) {
    for item in items {
        if item.menu_type == MenuType::Separator {
            continue;
        }
        if item.submenu.is_empty() {
            let action = gio::SimpleAction::new(&format!("item-{}", item.id), None);
            action.set_enabled(item.enabled);
            let addr = address.to_string();
            let id = item.id;
            let sender2 = sender.clone();
            action.connect_activate(move |_, _| {
                sender2.input(TrayInput::ActivateItem {
                    address: addr.clone(),
                    submenu_id: id,
                });
            });
            group.add_action(&action);
        } else {
            register_actions(&item.submenu, address, group, sender);
        }
    }
}
