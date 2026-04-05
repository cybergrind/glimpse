use std::collections::HashMap;
use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, gio, prelude::*},
};
use serde::Deserialize;

use crate::applets::tray::TrayConfig;

#[derive(Debug, Clone, Deserialize)]
struct TrayItemData {
    address: String,
    title: String,
    icon: String,
    status: String,
    item_is_menu: bool,
    menu_path: String,
    menu: Vec<TrayMenuItemData>,
}

#[derive(Debug, Clone, Deserialize)]
struct TrayMenuItemData {
    id: i32,
    label: String,
    enabled: bool,
    separator: bool,
    children: Vec<TrayMenuItemData>,
}

struct TrayItemState {
    button: gtk::Button,
    menu_path: String,
    popover: Option<gtk::PopoverMenu>,
}

pub struct Tray {
    config: TrayConfig,
    client: Arc<Client>,
    items: HashMap<String, TrayItemState>,
}

pub struct TrayInit {
    pub config: TrayConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum TrayInput {
    Update(Vec<TrayItemData>),
    Activate {
        address: String,
        x: i32,
        y: i32,
    },
    ActivateMenuItem {
        address: String,
        menu_path: String,
        submenu_id: i32,
    },
    ShowMenu(String),
    Unavailable,
}

#[relm4::component(pub)]
impl Component for Tray {
    type Init = TrayInit;
    type Input = TrayInput;
    type Output = ();
    type CommandOutput = TrayInput;

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
            client: init.client.clone(),
            items: HashMap::new(),
        };
        let widgets = view_output!();

        let client = init.client;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("tray applet: subscribing");
                    let mut sub = match client.subscribe("tray.items").await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("tray: failed to subscribe: {e}");
                            let _ = out.send(TrayInput::Unavailable);
                            return;
                        }
                    };
                    while let Some(event) = sub.next().await {
                        let items: Vec<TrayItemData> =
                            serde_json::from_value(event.data).unwrap_or_default();
                        let _ = out.send(TrayInput::Update(items));
                    }
                    let _ = out.send(TrayInput::Unavailable);
                })
                .drop_on_shutdown()
        });

        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            TrayInput::Update(new_items) => {
                tracing::info!(items = new_items.len(), "tray applet: update");
                let new_addresses: HashMap<&str, &TrayItemData> =
                    new_items.iter().map(|i| (i.address.as_str(), i)).collect();

                // Remove items no longer present.
                let to_remove: Vec<String> = self
                    .items
                    .keys()
                    .filter(|addr| !new_addresses.contains_key(addr.as_str()))
                    .cloned()
                    .collect();
                for addr in to_remove {
                    if let Some(state) = self.items.remove(&addr) {
                        if let Some(pop) = state.popover {
                            pop.unparent();
                        }
                        root.remove(&state.button);
                    }
                }

                // Add or update items.
                for item_data in &new_items {
                    if let Some(state) = self.items.get_mut(&item_data.address) {
                        // Update existing.
                        update_button_icon(&state.button, &item_data.icon, self.config.icon_size);
                        rebuild_menu(state, item_data, &sender);
                    } else {
                        // Add new.
                        let btn = make_button(&item_data.icon, self.config.icon_size);

                        let left = gtk::GestureClick::new();
                        left.set_button(1);
                        let addr = item_data.address.clone();
                        let is_menu = item_data.item_is_menu;
                        let left_sender = sender.clone();
                        left.connect_pressed(move |_, _, x, y| {
                            if is_menu {
                                left_sender.input(TrayInput::ShowMenu(addr.clone()));
                            } else {
                                left_sender.input(TrayInput::Activate {
                                    address: addr.clone(),
                                    x: x as i32,
                                    y: y as i32,
                                });
                            }
                        });
                        btn.add_controller(left);

                        let right = gtk::GestureClick::new();
                        right.set_button(3);
                        let addr = item_data.address.clone();
                        let right_sender = sender.clone();
                        right.connect_pressed(move |_, _, _, _| {
                            right_sender.input(TrayInput::ShowMenu(addr.clone()));
                        });
                        btn.add_controller(right);

                        root.append(&btn);
                        let mut state = TrayItemState {
                            button: btn,
                            menu_path: item_data.menu_path.clone(),
                            popover: None,
                        };
                        rebuild_menu(&mut state, item_data, &sender);
                        self.items.insert(item_data.address.clone(), state);
                    }
                }
            }
            TrayInput::Activate { address, x, y } => {
                let client = self.client.clone();
                tokio::spawn(async move {
                    let _ = client
                        .call(
                            "tray.activate",
                            serde_json::json!({"address": address, "x": x, "y": y}),
                        )
                        .await;
                });
            }
            TrayInput::ActivateMenuItem {
                address,
                menu_path,
                submenu_id,
            } => {
                let client = self.client.clone();
                tokio::spawn(async move {
                    let _ = client
                        .call(
                            "tray.activate_menu_item",
                            serde_json::json!({"address": address, "menu_path": menu_path, "submenu_id": submenu_id}),
                        )
                        .await;
                });
            }
            TrayInput::ShowMenu(address) => {
                if let Some(state) = self.items.get(&address) {
                    if let Some(popover) = &state.popover {
                        popover.popup();
                    }
                }
            }
            TrayInput::Unavailable => {
                tracing::warn!("tray applet: daemon unavailable");
            }
        }
    }
}

fn make_button(icon: &str, size: i32) -> gtk::Button {
    let image = gtk::Image::new();
    image.set_pixel_size(size);
    set_icon(&image, icon, size);
    let btn = gtk::Button::new();
    btn.set_child(Some(&image));
    btn.add_css_class("flat");
    btn.add_css_class("tray-item");
    btn
}

fn update_button_icon(btn: &gtk::Button, icon: &str, size: i32) {
    if let Some(image) = btn.child().and_downcast::<gtk::Image>() {
        set_icon(&image, icon, size);
    }
}

fn set_icon(image: &gtk::Image, icon: &str, size: i32) {
    if icon.starts_with('/') {
        image.set_from_file(Some(icon));
        image.set_pixel_size(size);
    } else if !icon.is_empty() {
        image.set_icon_name(Some(icon));
    } else {
        image.set_icon_name(Some("image-missing-symbolic"));
    }
}

fn rebuild_menu(
    state: &mut TrayItemState,
    item_data: &TrayItemData,
    sender: &ComponentSender<Tray>,
) {
    if item_data.menu.is_empty() {
        return;
    }

    let gio_menu = build_gio_menu(&item_data.menu);
    let group = gio::SimpleActionGroup::new();
    register_actions(
        &item_data.menu,
        &item_data.address,
        &item_data.menu_path,
        &group,
        sender,
    );
    state.button.insert_action_group("tray", Some(&group));

    let popover = gtk::PopoverMenu::from_model(Some(&gio_menu));
    popover.insert_action_group("tray", Some(&group));
    popover.set_parent(&state.button);

    if let Some(old) = state.popover.replace(popover) {
        old.popdown();
        old.unparent();
    }
}

fn build_gio_menu(items: &[TrayMenuItemData]) -> gio::Menu {
    let menu = gio::Menu::new();
    let mut section = gio::Menu::new();

    for item in items {
        if item.separator {
            menu.append_section(None, &section);
            section = gio::Menu::new();
            continue;
        }
        if !item.children.is_empty() {
            let submenu = build_gio_menu(&item.children);
            section.append_submenu(Some(&item.label), &submenu);
        } else {
            let action = format!("tray.item-{}", item.id);
            section.append(Some(&item.label), Some(&action));
        }
    }
    menu.append_section(None, &section);
    menu
}

fn register_actions(
    items: &[TrayMenuItemData],
    address: &str,
    menu_path: &str,
    group: &gio::SimpleActionGroup,
    sender: &ComponentSender<Tray>,
) {
    for item in items {
        if item.separator {
            continue;
        }
        if item.children.is_empty() {
            let action = gio::SimpleAction::new(&format!("item-{}", item.id), None);
            action.set_enabled(item.enabled);
            let addr = address.to_owned();
            let mp = menu_path.to_owned();
            let id = item.id;
            let s = sender.clone();
            action.connect_activate(move |_, _| {
                s.input(TrayInput::ActivateMenuItem {
                    address: addr.clone(),
                    menu_path: mp.clone(),
                    submenu_id: id,
                });
            });
            group.add_action(&action);
        } else {
            register_actions(&item.children, address, menu_path, group, sender);
        }
    }
}
