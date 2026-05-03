#![allow(unused_assignments)]

use std::collections::HashMap;

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, gio, prelude::*},
};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{
    panels::applets::AppletConfig,
    services::{
        framework::ServiceCommand,
        tray::{
            TrayHandle,
            model::{
                Item, MenuItem, MenuItemKind, MenuToggleState, MenuToggleType, Snapshot, Status,
            },
            protocol::{Command, State},
        },
    },
};

use super::components::item::{
    Init as ItemInit, Input as ItemInput, Output as ItemOutput, TrayItem, ViewModel, icon_to_gicon,
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub icon_size: i32,
    pub show_passive: bool,
}

impl Config {
    pub fn from_raw(raw: &Option<AppletConfig>) -> Self {
        let Some(raw) = raw else {
            return Self::default();
        };

        match raw.settings.clone().try_into() {
            Ok(config) => config,
            Err(error) => {
                tracing::warn!(?error, "invalid tray applet config, using defaults");
                Self::default()
            }
        }
    }

    fn icon_size(&self) -> i32 {
        self.icon_size.clamp(12, 32)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            icon_size: 16,
            show_passive: false,
        }
    }
}

pub struct Applet {
    config: Config,
    service: TrayHandle,
    snapshot: Snapshot,
    items: HashMap<String, ItemState>,
    subscription_cancel: CancellationToken,
}

struct ItemState {
    controller: Controller<TrayItem>,
    view: ViewModel,
    icon_size: i32,
    menu_path: String,
    menu: Vec<MenuItem>,
    popover: Option<gtk::PopoverMenu>,
}

#[derive(Debug)]
pub struct Init {
    pub service: TrayHandle,
    pub config: Config,
}

#[derive(Debug)]
pub enum Input {
    ServiceStateChanged(State),
    Reconfigure(Config),
    PrimaryClick {
        address: String,
        x: i32,
        y: i32,
    },
    MiddleClick {
        address: String,
        x: i32,
        y: i32,
    },
    ContextClick {
        address: String,
        x: i32,
        y: i32,
    },
    Scroll {
        address: String,
        delta: i32,
        orientation: crate::services::tray::protocol::ScrollOrientation,
    },
    MenuItemInvoked {
        address: String,
        menu_path: String,
        item_id: i32,
    },
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickKind {
    Primary,
    Middle,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ClickOutcome {
    PopupLocalMenu,
    ServiceCommand(Command),
}

#[relm4::component(pub)]
impl Component for Applet {
    type Init = Init;
    type Input = Input;
    type Output = ();
    type CommandOutput = Input;

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 0,
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Applet {
            config: init.config,
            service: init.service,
            snapshot: Snapshot::default(),
            items: HashMap::new(),
            subscription_cancel: CancellationToken::new(),
        };

        let service = model.service.clone();
        let cancel = model.subscription_cancel.clone();
        let subscription_sender = sender.clone();
        relm4::spawn(async move {
            let mut sub = service.subscribe();
            subscription_sender.input(Input::ServiceStateChanged(sub.borrow().clone()));

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    changed = sub.changed() => {
                        if changed.is_err() {
                            break;
                        }

                        subscription_sender.input(Input::ServiceStateChanged(sub.borrow().clone()));
                    }
                }
            }

            subscription_sender.input(Input::Unavailable);
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(message, sender, root);
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            Input::ServiceStateChanged(state) => {
                self.snapshot = state.snapshot;
                self.sync_items(root, &sender);
            }
            Input::Reconfigure(config) => {
                self.config = config;
                self.sync_items(root, &sender);
            }
            Input::PrimaryClick { address, x, y } => {
                self.handle_click(&address, ClickKind::Primary, x, y);
            }
            Input::MiddleClick { address, x, y } => {
                self.handle_click(&address, ClickKind::Middle, x, y);
            }
            Input::ContextClick { address, x, y } => {
                self.handle_click(&address, ClickKind::Context, x, y);
            }
            Input::Scroll {
                address,
                delta,
                orientation,
            } => self.send_command(Command::Scroll {
                address,
                delta,
                orientation,
            }),
            Input::MenuItemInvoked {
                address,
                menu_path,
                item_id,
            } => self.send_command(Command::ActivateMenuItem {
                address,
                menu_path,
                item_id,
            }),
            Input::Unavailable => {
                tracing::warn!("tray applet: tray service unavailable");
            }
        }
    }
}

impl Applet {
    fn sync_items(&mut self, root: &gtk::Box, sender: &ComponentSender<Applet>) {
        let visible_items = self
            .snapshot
            .items
            .iter()
            .filter(|item| self.config.show_passive || item.status != Status::Passive)
            .cloned()
            .collect::<Vec<_>>();
        let visible_addresses = visible_items
            .iter()
            .map(|item| item.address.as_str())
            .collect::<std::collections::HashSet<_>>();

        let to_remove = self
            .items
            .keys()
            .filter(|address| !visible_addresses.contains(address.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        for address in to_remove {
            if let Some(state) = self.items.remove(&address) {
                detach_popover(state.popover);
                root.remove(state.controller.widget());
            }
        }

        for item in &visible_items {
            if let Some(state) = self.items.get_mut(&item.address) {
                let icon_size = self.config.icon_size();
                if state.icon_size != icon_size {
                    state.controller.emit(ItemInput::SetIconSize(icon_size));
                    state.icon_size = icon_size;
                }

                let next_view = ViewModel::from(item);
                if state.view != next_view {
                    state.controller.emit(ItemInput::Update(next_view.clone()));
                    state.view = next_view;
                }

                rebuild_menu(state, item, sender);
            } else {
                let address = item.address.clone();
                let view = ViewModel::from(item);
                let icon_size = self.config.icon_size();
                let controller = TrayItem::builder()
                    .launch(ItemInit {
                        view: view.clone(),
                        icon_size,
                    })
                    .forward(sender.input_sender(), move |output| match output {
                        ItemOutput::PrimaryClick { x, y } => Input::PrimaryClick {
                            address: address.clone(),
                            x,
                            y,
                        },
                        ItemOutput::MiddleClick { x, y } => Input::MiddleClick {
                            address: address.clone(),
                            x,
                            y,
                        },
                        ItemOutput::ContextClick { x, y } => Input::ContextClick {
                            address: address.clone(),
                            x,
                            y,
                        },
                        ItemOutput::Scroll { delta, orientation } => Input::Scroll {
                            address: address.clone(),
                            delta,
                            orientation,
                        },
                    });
                root.append(controller.widget());

                let mut state = ItemState {
                    controller,
                    view,
                    icon_size,
                    menu_path: String::new(),
                    menu: Vec::new(),
                    popover: None,
                };
                rebuild_menu(&mut state, item, sender);
                self.items.insert(item.address.clone(), state);
            }
        }

        let mut previous: Option<gtk::Widget> = None;
        for item in &visible_items {
            let Some(state) = self.items.get(&item.address) else {
                continue;
            };
            root.reorder_child_after(state.controller.widget(), previous.as_ref());
            previous = Some(state.controller.widget().clone().upcast());
        }
    }

    fn handle_click(&self, address: &str, click: ClickKind, x: i32, y: i32) {
        let Some(item) = self.item(address) else {
            tracing::debug!(address, "tray applet: ignoring click for unknown item");
            return;
        };

        match command_for_click(item, click, x, y) {
            ClickOutcome::PopupLocalMenu => {
                self.send_command(Command::AboutToShowMenu {
                    address: item.address.clone(),
                    menu_path: item.menu_path.clone(),
                    item_id: 0,
                });
                if let Some(state) = self
                    .items
                    .get(address)
                    .and_then(|state| state.popover.as_ref())
                {
                    state.popup();
                }
            }
            ClickOutcome::ServiceCommand(command) => self.send_command(command),
        }
    }

    fn item(&self, address: &str) -> Option<&Item> {
        self.snapshot
            .items
            .iter()
            .find(|item| item.address == address)
    }

    fn send_command(&self, command: Command) {
        let service = self.service.clone();
        relm4::spawn(async move {
            if let Err(error) = service.send(ServiceCommand::Command(command)).await {
                tracing::warn!(error = %error, "tray applet: failed to send service command");
            }
        });
    }
}

impl Drop for Applet {
    fn drop(&mut self) {
        self.subscription_cancel.cancel();
        for (_, state) in self.items.drain() {
            detach_popover(state.popover);
        }
    }
}

fn rebuild_menu(state: &mut ItemState, item: &Item, sender: &ComponentSender<Applet>) {
    if state.menu_path == item.menu_path && state.menu == item.menu {
        return;
    }

    let was_visible = state
        .popover
        .as_ref()
        .is_some_and(gtk::prelude::WidgetExt::is_visible);

    state
        .controller
        .widget()
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
        .controller
        .widget()
        .insert_action_group("tray", Some(&action_group));

    let popover = gtk::PopoverMenu::from_model(Some(&gio_menu));
    popover.insert_action_group("tray", Some(&action_group));
    popover.set_parent(state.controller.widget());
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

fn build_gio_menu(items: &[MenuItem]) -> gio::Menu {
    let menu = gio::Menu::new();
    let mut section = gio::Menu::new();
    let mut has_section_items = false;

    for item in items.iter().filter(|item| item.visible) {
        if matches!(item.kind, MenuItemKind::Separator) {
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

fn menu_label(item: &MenuItem) -> String {
    let prefix = match (item.toggle_type, item.toggle_state) {
        (MenuToggleType::Checkmark, MenuToggleState::On) => "✓ ",
        (MenuToggleType::Checkmark, MenuToggleState::Off) => "  ",
        (MenuToggleType::Radio, MenuToggleState::On) => "◉ ",
        (MenuToggleType::Radio, MenuToggleState::Off) => "○ ",
        _ => "",
    };

    format!("{prefix}{}", item.label)
}

fn register_actions(
    items: &[MenuItem],
    address: &str,
    menu_path: &str,
    group: &gio::SimpleActionGroup,
    sender: &ComponentSender<Applet>,
) {
    for item in items.iter().filter(|item| item.visible) {
        if matches!(item.kind, MenuItemKind::Separator) {
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
        let item_id = item.id;
        let sender = sender.clone();
        action.connect_activate(move |_, _| {
            sender.input(Input::MenuItemInvoked {
                address: address.clone(),
                menu_path: menu_path.clone(),
                item_id,
            });
        });
        group.add_action(&action);
    }
}

fn has_visible_menu_items(items: &[MenuItem]) -> bool {
    items.iter().any(|item| {
        item.visible
            && (matches!(item.kind, MenuItemKind::Standard)
                || has_visible_menu_items(&item.children))
    })
}

fn command_for_click(item: &Item, click: ClickKind, x: i32, y: i32) -> ClickOutcome {
    match click {
        ClickKind::Primary if !item.item_is_menu => {
            ClickOutcome::ServiceCommand(Command::Activate {
                address: item.address.clone(),
                x,
                y,
            })
        }
        ClickKind::Middle => ClickOutcome::ServiceCommand(Command::SecondaryActivate {
            address: item.address.clone(),
            x,
            y,
        }),
        ClickKind::Primary | ClickKind::Context => {
            if has_visible_menu_items(&item.menu) {
                ClickOutcome::PopupLocalMenu
            } else {
                ClickOutcome::ServiceCommand(Command::OpenContextMenu {
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
    use super::*;
    use crate::services::tray::model::{Category, Icon, MenuDisposition, MenuToggleState};

    #[test]
    fn normal_item_left_click_activates() {
        assert_eq!(
            command_for_click(&test_item(false, vec![]), ClickKind::Primary, 4, 8),
            ClickOutcome::ServiceCommand(Command::Activate {
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
                    vec![MenuItem {
                        id: 1,
                        label: "Open".into(),
                        enabled: true,
                        visible: true,
                        kind: MenuItemKind::Standard,
                        icon: None,
                        shortcut: None,
                        toggle_type: MenuToggleType::CannotBeToggled,
                        toggle_state: MenuToggleState::Indeterminate,
                        children_display: None,
                        disposition: MenuDisposition::Normal,
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
            command_for_click(&test_item(true, vec![]), ClickKind::Context, 11, 13),
            ClickOutcome::ServiceCommand(Command::OpenContextMenu {
                address: "org.example.App".into(),
                x: 11,
                y: 13,
            })
        );
    }

    #[test]
    fn middle_click_uses_secondary_activate() {
        assert_eq!(
            command_for_click(&test_item(false, vec![]), ClickKind::Middle, 2, 3),
            ClickOutcome::ServiceCommand(Command::SecondaryActivate {
                address: "org.example.App".into(),
                x: 2,
                y: 3,
            })
        );
    }

    #[test]
    fn filters_separator_only_menus_as_empty() {
        assert!(!has_visible_menu_items(&[MenuItem {
            id: 1,
            label: String::new(),
            enabled: false,
            visible: true,
            kind: MenuItemKind::Separator,
            icon: None,
            shortcut: None,
            toggle_type: MenuToggleType::CannotBeToggled,
            toggle_state: MenuToggleState::Indeterminate,
            children_display: None,
            disposition: MenuDisposition::Normal,
            children: Vec::new(),
        }]));
    }

    fn test_item(item_is_menu: bool, menu: Vec<MenuItem>) -> Item {
        Item {
            address: "org.example.App".into(),
            id: "example".into(),
            title: "Example".into(),
            status: Status::Active,
            category: Category::ApplicationStatus,
            item_is_menu,
            menu_path: "/MenuBar".into(),
            icon_theme_path: None,
            icon: Some(Icon::Name("example-symbolic".into())),
            overlay_icon: None,
            attention_icon: None,
            attention_movie_name: None,
            tooltip: None,
            menu,
        }
    }
}
