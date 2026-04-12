use std::collections::HashMap;

use glimpse::tray::{
    TrayServiceHandle,
    protocol::{
        TrayIcon, TrayItem, TrayMenuItem, TrayMenuItemKind, TrayMenuToggleState,
        TrayMenuToggleType, TrayServiceCommand, TrayServiceState, TraySnapshot,
    },
};
use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, gdk, gio, glib, prelude::*},
};

use crate::applets::tray::TrayConfig;

struct TrayItemState {
    button: gtk::Button,
    menu_path: String,
    menu: Vec<TrayMenuItem>,
    popover: Option<gtk::PopoverMenu>,
}

pub struct Tray {
    config: TrayConfig,
    service: TrayServiceHandle,
    snapshot: TraySnapshot,
    items: HashMap<String, TrayItemState>,
}

pub struct TrayInit {
    pub config: TrayConfig,
    pub service: TrayServiceHandle,
}

#[derive(Debug, Clone)]
pub enum TrayInput {
    ServiceState(TrayServiceState),
    PrimaryClick {
        address: String,
        x: i32,
        y: i32,
    },
    SecondaryClick {
        address: String,
        x: i32,
        y: i32,
    },
    MenuItemInvoked {
        address: String,
        menu_path: String,
        submenu_id: i32,
    },
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickKind {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClickOutcome {
    PopupLocalMenu,
    ServiceCommand(TrayServiceCommand),
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
            service: init.service.clone(),
            snapshot: TraySnapshot::default(),
            items: HashMap::new(),
        };
        let widgets = view_output!();

        let service = init.service;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("tray applet: subscribing to tray service");
                    let mut state_rx = service.subscribe();
                    let _ = out.send(TrayInput::ServiceState(state_rx.borrow().clone()));

                    loop {
                        if state_rx.changed().await.is_err() {
                            break;
                        }
                        let _ = out.send(TrayInput::ServiceState(state_rx.borrow().clone()));
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
            TrayInput::ServiceState(state) => {
                self.snapshot = state.snapshot;
                self.sync_items(root, &sender);
            }
            TrayInput::PrimaryClick { address, x, y } => {
                self.handle_click(&address, ClickKind::Primary, x, y, sender);
            }
            TrayInput::SecondaryClick { address, x, y } => {
                self.handle_click(&address, ClickKind::Secondary, x, y, sender);
            }
            TrayInput::MenuItemInvoked {
                address,
                menu_path,
                submenu_id,
            } => {
                self.send_command(
                    sender,
                    TrayServiceCommand::ActivateMenuItem {
                        address,
                        menu_path,
                        submenu_id,
                    },
                );
            }
            TrayInput::Unavailable => {
                tracing::warn!("tray applet: tray service unavailable");
            }
        }
    }
}

impl Tray {
    fn sync_items(&mut self, root: &gtk::Box, sender: &ComponentSender<Tray>) {
        let new_addresses: HashMap<&str, &TrayItem> = self
            .snapshot
            .items
            .iter()
            .map(|item| (item.address.as_str(), item))
            .collect();

        let to_remove = self
            .items
            .keys()
            .filter(|address| !new_addresses.contains_key(address.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for address in to_remove {
            if let Some(state) = self.items.remove(&address) {
                detach_popover(state.popover);
                root.remove(&state.button);
            }
        }

        for item in &self.snapshot.items {
            if let Some(state) = self.items.get_mut(&item.address) {
                update_button(&state.button, item, self.config.icon_size);
                rebuild_menu(state, item, sender);
            } else {
                let button = build_button(item, self.config.icon_size, sender);
                root.append(&button);

                let mut state = TrayItemState {
                    button,
                    menu_path: String::new(),
                    menu: Vec::new(),
                    popover: None,
                };
                rebuild_menu(&mut state, item, sender);
                self.items.insert(item.address.clone(), state);
            }
        }

        let mut previous: Option<gtk::Widget> = None;
        for item in &self.snapshot.items {
            let Some(state) = self.items.get(&item.address) else {
                continue;
            };
            root.reorder_child_after(&state.button, previous.as_ref());
            previous = Some(state.button.clone().upcast());
        }
    }

    fn handle_click(
        &self,
        address: &str,
        click: ClickKind,
        x: i32,
        y: i32,
        sender: ComponentSender<Tray>,
    ) {
        let Some(item) = self.item(address) else {
            tracing::debug!(address, "tray applet: ignoring click for unknown item");
            return;
        };

        match command_for_click(item, click, x, y) {
            ClickOutcome::PopupLocalMenu => {
                self.send_command(
                    sender,
                    TrayServiceCommand::AboutToShowMenu {
                        address: item.address.clone(),
                        menu_path: item.menu_path.clone(),
                        item_id: 0,
                    },
                );
                if let Some(state) = self.items.get(address) {
                    if let Some(popover) = &state.popover {
                        popover.popup();
                    }
                }
            }
            ClickOutcome::ServiceCommand(command) => self.send_command(sender, command),
        }
    }

    fn item(&self, address: &str) -> Option<&TrayItem> {
        self.snapshot
            .items
            .iter()
            .find(|item| item.address == address)
    }

    fn send_command(&self, sender: ComponentSender<Tray>, command: TrayServiceCommand) {
        let service = self.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    if let Err(error) = service.send(command).await {
                        tracing::warn!(error = %error, "tray applet: failed to send tray service command");
                    }
                })
                .drop_on_shutdown()
        });
    }
}

fn build_button(item: &TrayItem, size: i32, sender: &ComponentSender<Tray>) -> gtk::Button {
    let image = gtk::Image::new();
    image.set_pixel_size(size);
    set_image_icon(&image, item, size);

    let button = gtk::Button::new();
    button.set_child(Some(&image));
    button.add_css_class("flat");
    button.add_css_class("tray-item");
    button.set_tooltip_text(button_tooltip(item).as_deref());

    let left = gtk::GestureClick::new();
    left.set_button(1);
    let address = item.address.clone();
    let left_sender = sender.clone();
    left.connect_pressed(move |_, _, x, y| {
        left_sender.input(TrayInput::PrimaryClick {
            address: address.clone(),
            x: x as i32,
            y: y as i32,
        });
    });
    button.add_controller(left);

    let right = gtk::GestureClick::new();
    right.set_button(3);
    let address = item.address.clone();
    let right_sender = sender.clone();
    right.connect_pressed(move |_, _, x, y| {
        right_sender.input(TrayInput::SecondaryClick {
            address: address.clone(),
            x: x as i32,
            y: y as i32,
        });
    });
    button.add_controller(right);

    button
}

fn update_button(button: &gtk::Button, item: &TrayItem, size: i32) {
    if let Some(image) = button.child().and_downcast::<gtk::Image>() {
        set_image_icon(&image, item, size);
    }
    button.set_tooltip_text(button_tooltip(item).as_deref());
}

fn button_tooltip(item: &TrayItem) -> Option<String> {
    if let Some(tooltip) = &item.tooltip {
        if !tooltip.description.is_empty() {
            return Some(tooltip.description.clone());
        }
        if !tooltip.title.is_empty() {
            return Some(tooltip.title.clone());
        }
    }

    (!item.title.is_empty()).then(|| item.title.clone())
}

fn set_image_icon(image: &gtk::Image, item: &TrayItem, size: i32) {
    image.set_pixel_size(size);

    match item_icon_paintable(item, size) {
        Some(paintable) => image.set_paintable(Some(&paintable)),
        None => match item.icon.as_ref().and_then(icon_to_gicon) {
            Some(gicon) => image.set_from_gicon(&gicon),
            None => image.set_icon_name(Some("image-missing-symbolic")),
        },
    }
}

fn item_icon_paintable(item: &TrayItem, size: i32) -> Option<gdk::Paintable> {
    let TrayIcon::Name(name) = item.icon.as_ref()? else {
        return None;
    };

    let icon_theme_path = item
        .icon_theme_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())?;
    let display = gdk::Display::default()?;
    let theme = gtk::IconTheme::for_display(&display);
    let theme_path = std::path::Path::new(icon_theme_path);
    if !theme
        .search_path()
        .iter()
        .any(|existing| existing == theme_path)
    {
        theme.add_search_path(theme_path);
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

fn icon_to_gicon(icon: &TrayIcon) -> Option<gio::Icon> {
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

fn rebuild_menu(state: &mut TrayItemState, item: &TrayItem, sender: &ComponentSender<Tray>) {
    if !menu_state_changed(&state.menu_path, &state.menu, item) {
        return;
    }

    let was_visible = state
        .popover
        .as_ref()
        .is_some_and(gtk::prelude::WidgetExt::is_visible);

    state
        .button
        .insert_action_group("tray", Option::<&gio::SimpleActionGroup>::None);
    detach_popover(state.popover.take());
    state.menu_path = item.menu_path.clone();
    state.menu = item.menu.clone();

    if !has_visible_menu_items(&item.menu) {
        return;
    }

    let gio_menu = build_gio_menu(&item.menu);
    let action_group = gio::SimpleActionGroup::new();
    register_actions(
        &item.menu,
        &item.address,
        &item.menu_path,
        &action_group,
        sender,
    );
    state
        .button
        .insert_action_group("tray", Some(&action_group));

    let popover = gtk::PopoverMenu::from_model(Some(&gio_menu));
    popover.insert_action_group("tray", Some(&action_group));
    popover.set_parent(&state.button);
    if was_visible {
        popover.popup();
    }
    state.popover = Some(popover);
}

fn detach_popover(popover: Option<gtk::PopoverMenu>) {
    if let Some(popover) = popover {
        popover.popdown();
        popover.unparent();
    }
}

fn build_gio_menu(items: &[TrayMenuItem]) -> gio::Menu {
    let menu = gio::Menu::new();
    let mut section = gio::Menu::new();
    let mut has_section_items = false;

    for item in items.iter().filter(|item| item.visible) {
        if matches!(item.kind, TrayMenuItemKind::Separator) {
            if has_section_items {
                menu.append_section(None, &section);
                section = gio::Menu::new();
                has_section_items = false;
            }
            continue;
        }

        let menu_item = if has_visible_menu_items(&item.children) {
            gio::MenuItem::new_submenu(Some(&menu_label(item)), &build_gio_menu(&item.children))
        } else {
            gio::MenuItem::new(
                Some(&menu_label(item)),
                Some(&format!("tray.item-{}", item.id)),
            )
        };

        if let Some(icon) = item.icon.as_ref().and_then(icon_to_gicon) {
            menu_item.set_icon(&icon);
        }

        section.append_item(&menu_item);
        has_section_items = true;
    }

    if has_section_items {
        menu.append_section(None, &section);
    }

    menu
}

fn menu_label(item: &TrayMenuItem) -> String {
    let prefix = match (item.toggle_type, item.toggle_state) {
        (TrayMenuToggleType::Checkmark, TrayMenuToggleState::On) => "✓ ",
        (TrayMenuToggleType::Checkmark, TrayMenuToggleState::Off) => "  ",
        (TrayMenuToggleType::Radio, TrayMenuToggleState::On) => "◉ ",
        (TrayMenuToggleType::Radio, TrayMenuToggleState::Off) => "○ ",
        _ => "",
    };

    format!("{prefix}{}", item.label)
}

fn register_actions(
    items: &[TrayMenuItem],
    address: &str,
    menu_path: &str,
    group: &gio::SimpleActionGroup,
    sender: &ComponentSender<Tray>,
) {
    for item in items.iter().filter(|item| item.visible) {
        if matches!(item.kind, TrayMenuItemKind::Separator) {
            continue;
        }

        if has_visible_menu_items(&item.children) {
            register_actions(&item.children, address, menu_path, group, sender);
            continue;
        }

        let action = gio::SimpleAction::new(&format!("item-{}", item.id), None);
        action.set_enabled(item.enabled);
        let address = address.to_owned();
        let menu_path = menu_path.to_owned();
        let submenu_id = item.id;
        let sender = sender.clone();
        action.connect_activate(move |_, _| {
            sender.input(TrayInput::MenuItemInvoked {
                address: address.clone(),
                menu_path: menu_path.clone(),
                submenu_id,
            });
        });
        group.add_action(&action);
    }
}

fn has_visible_menu_items(items: &[TrayMenuItem]) -> bool {
    items.iter().any(|item| {
        item.visible
            && (!matches!(item.kind, TrayMenuItemKind::Separator)
                || has_visible_menu_items(&item.children))
    })
}

fn menu_state_changed(menu_path: &str, menu: &[TrayMenuItem], item: &TrayItem) -> bool {
    menu_path != item.menu_path || menu != item.menu
}

fn command_for_click(item: &TrayItem, click: ClickKind, x: i32, y: i32) -> ClickOutcome {
    match click {
        ClickKind::Primary if !item.item_is_menu => {
            ClickOutcome::ServiceCommand(TrayServiceCommand::Activate {
                address: item.address.clone(),
                x,
                y,
            })
        }
        ClickKind::Primary | ClickKind::Secondary => {
            if has_visible_menu_items(&item.menu) {
                ClickOutcome::PopupLocalMenu
            } else {
                ClickOutcome::ServiceCommand(TrayServiceCommand::OpenContextMenu {
                    address: item.address.clone(),
                    x,
                    y,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use glimpse::tray::protocol::{TrayCategory, TrayMenuDisposition, TrayStatus, TrayTooltip};

    use super::*;

    #[test]
    fn normal_item_left_click_activates() {
        assert_eq!(
            command_for_click(&test_item(false, vec![]), ClickKind::Primary, 4, 8),
            ClickOutcome::ServiceCommand(TrayServiceCommand::Activate {
                address: "org.example.App".into(),
                x: 4,
                y: 8,
            })
        );
    }

    #[test]
    fn item_is_menu_left_click_prefers_local_menu() {
        assert_eq!(
            command_for_click(
                &test_item(
                    true,
                    vec![TrayMenuItem {
                        id: 1,
                        label: "Open".into(),
                        enabled: true,
                        visible: true,
                        kind: TrayMenuItemKind::Standard,
                        icon: None,
                        shortcut: None,
                        toggle_type: TrayMenuToggleType::CannotBeToggled,
                        toggle_state: TrayMenuToggleState::Indeterminate,
                        children_display: None,
                        disposition: TrayMenuDisposition::Normal,
                        children: Vec::new(),
                    }]
                ),
                ClickKind::Primary,
                4,
                8,
            ),
            ClickOutcome::PopupLocalMenu
        );
    }

    #[test]
    fn right_click_without_local_menu_uses_remote_context_menu() {
        assert_eq!(
            command_for_click(&test_item(true, vec![]), ClickKind::Secondary, 11, 13),
            ClickOutcome::ServiceCommand(TrayServiceCommand::OpenContextMenu {
                address: "org.example.App".into(),
                x: 11,
                y: 13,
            })
        );
    }

    #[test]
    fn unchanged_menu_state_does_not_require_rebuild() {
        let menu = vec![TrayMenuItem {
            id: 1,
            label: "Open".into(),
            enabled: true,
            visible: true,
            kind: TrayMenuItemKind::Standard,
            icon: None,
            shortcut: None,
            toggle_type: TrayMenuToggleType::CannotBeToggled,
            toggle_state: TrayMenuToggleState::Indeterminate,
            children_display: None,
            disposition: TrayMenuDisposition::Normal,
            children: Vec::new(),
        }];

        assert!(!menu_state_changed(
            "/MenuBar",
            &menu,
            &test_item_with_menu("/MenuBar", menu.clone()),
        ));
    }

    #[test]
    fn changed_menu_state_requires_rebuild() {
        assert!(menu_state_changed(
            "/MenuBar",
            &[],
            &test_item_with_menu(
                "/NewMenu",
                vec![TrayMenuItem {
                    id: 1,
                    label: "Open".into(),
                    enabled: true,
                    visible: true,
                    kind: TrayMenuItemKind::Standard,
                    icon: None,
                    shortcut: None,
                    toggle_type: TrayMenuToggleType::CannotBeToggled,
                    toggle_state: TrayMenuToggleState::Indeterminate,
                    children_display: None,
                    disposition: TrayMenuDisposition::Normal,
                    children: Vec::new(),
                }],
            ),
        ));
    }

    fn test_item(item_is_menu: bool, menu: Vec<TrayMenuItem>) -> TrayItem {
        test_item_with_menu_path(item_is_menu, "/MenuBar", menu)
    }

    fn test_item_with_menu(menu_path: &str, menu: Vec<TrayMenuItem>) -> TrayItem {
        test_item_with_menu_path(true, menu_path, menu)
    }

    fn test_item_with_menu_path(
        item_is_menu: bool,
        menu_path: &str,
        menu: Vec<TrayMenuItem>,
    ) -> TrayItem {
        TrayItem {
            address: "org.example.App".into(),
            id: "example".into(),
            title: "Example".into(),
            status: TrayStatus::Active,
            category: TrayCategory::ApplicationStatus,
            item_is_menu,
            menu_path: menu_path.into(),
            icon_theme_path: None,
            icon: Some(TrayIcon::Name("example-symbolic".into())),
            overlay_icon: None,
            attention_icon: None,
            attention_movie_name: None,
            tooltip: Some(TrayTooltip {
                title: "Example".into(),
                description: "Tooltip".into(),
                icon: None,
            }),
            menu,
        }
    }
}
