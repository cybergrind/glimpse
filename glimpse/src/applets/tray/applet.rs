use std::collections::HashMap;

use glimpse::tray::{
    TrayServiceHandle,
    protocol::{
        TrayItem, TrayMenuItem, TrayMenuItemKind, TrayMenuToggleState, TrayMenuToggleType,
        TrayServiceCommand, TrayServiceState, TraySnapshot,
    },
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, gio, prelude::*},
};

use crate::applets::tray::{
    TrayConfig,
    components::tray_button::{
        TrayButton, TrayButtonInit, TrayButtonInput, TrayButtonOutput, TrayButtonView,
        icon_to_gicon,
    },
};

struct TrayItemState {
    controller: Controller<TrayButton>,
    view: TrayButtonView,
    icon_size: i32,
    item: TrayItem,
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
    Reconfigure(TrayConfig),
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
        _root: Self::Root,
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
            TrayInput::Reconfigure(config) => {
                self.config = config;
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
                root.remove(state.controller.widget());
            }
        }

        for item in &self.snapshot.items {
            if let Some(state) = self.items.get_mut(&item.address) {
                if state.icon_size != self.config.icon_size {
                    state
                        .controller
                        .emit(TrayButtonInput::SetIconSize(self.config.icon_size));
                    state.icon_size = self.config.icon_size;
                }
                if item_view_changed(&state.view, item) {
                    let next_view = TrayButtonView::from(item);
                    state
                        .controller
                        .emit(TrayButtonInput::Update(next_view.clone()));
                    state.view = next_view;
                }
                state.item = item.clone();
                rebuild_menu(state, item, sender);
            } else {
                let address = item.address.clone();
                let view = TrayButtonView::from(item);
                let controller = TrayButton::builder()
                    .launch(TrayButtonInit {
                        view: view.clone(),
                        icon_size: self.config.icon_size,
                    })
                    .forward(sender.input_sender(), move |output| match output {
                        TrayButtonOutput::PrimaryClick { x, y } => TrayInput::PrimaryClick {
                            address: address.clone(),
                            x,
                            y,
                        },
                        TrayButtonOutput::SecondaryClick { x, y } => TrayInput::SecondaryClick {
                            address: address.clone(),
                            x,
                            y,
                        },
                    });
                root.append(controller.widget());

                let mut state = TrayItemState {
                    controller,
                    view,
                    icon_size: self.config.icon_size,
                    item: item.clone(),
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
            root.reorder_child_after(state.controller.widget(), previous.as_ref());
            previous = Some(state.controller.widget().clone().upcast());
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

fn rebuild_menu(state: &mut TrayItemState, item: &TrayItem, sender: &ComponentSender<Tray>) {
    if !menu_state_changed(&state.menu_path, &state.menu, item) {
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

fn item_view_changed(view: &TrayButtonView, item: &TrayItem) -> bool {
    *view != TrayButtonView::from(item)
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
    use glimpse::tray::protocol::{
        TrayCategory, TrayIcon, TrayMenuDisposition, TrayStatus, TrayTooltip,
    };

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

    #[test]
    fn unchanged_item_view_does_not_require_button_update() {
        let item = test_item(false, vec![]);
        assert!(!item_view_changed(&TrayButtonView::from(&item), &item));
    }

    #[test]
    fn menu_only_change_does_not_require_button_update() {
        let current = test_item(false, vec![]);
        let mut next = current.clone();
        next.menu = vec![TrayMenuItem {
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

        assert!(!item_view_changed(&TrayButtonView::from(&current), &next));
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
