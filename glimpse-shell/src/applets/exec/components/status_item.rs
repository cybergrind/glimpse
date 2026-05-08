#![allow(unused_assignments)]

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gio, glib, prelude::*},
};

use crate::applets::exec::{
    protocol::{
        EventKind, EventPayload, EventSource, MouseButton, StatusItem as StatusItemModel,
        StatusMenuItem,
    },
    renderer::apply_icon_to_image,
};

pub struct StatusItem {
    item: StatusItemModel,
    has_popover: bool,
    image: gtk::Image,
    context_menu: gtk::PopoverMenu,
    action_group: gio::SimpleActionGroup,
}

#[derive(Debug, Clone)]
pub struct Init {
    pub item: StatusItemModel,
    pub has_popover: bool,
}

#[derive(Debug)]
pub enum Input {
    Click(u32),
    Scroll(f64),
    Reconfigure {
        item: StatusItemModel,
        has_popover: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    TogglePopover,
    ContextMenu,
    Event(EventPayload),
    Activate(Option<EventPayload>),
}

#[relm4::component(pub)]
impl SimpleComponent for StatusItem {
    type Init = Init;
    type Input = Input;
    type Output = Output;

    view! {
        gtk::Box {
            add_css_class: "exec-status-item",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            set_valign: gtk::Align::Center,
            #[watch]
            set_tooltip_text: model.item.tooltip.as_deref(),

            add_controller = gtk::GestureClick {
                set_button: 0,
                connect_pressed[sender] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    sender.input(Input::Click(gesture.current_button()));
                }
            },

            add_controller = gtk::EventControllerScroll {
                set_flags: gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
                connect_scroll[sender] => move |_, _, delta_y| {
                    sender.input(Input::Scroll(delta_y));
                    glib::Propagation::Proceed
                }
            },

            #[name = "image"]
            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                #[watch]
                set_visible: model.item.icon.is_some(),
            },

            gtk::Label {
                add_css_class: "exec-status-label",
                set_valign: gtk::Align::Center,
                set_single_line_mode: true,
                set_ellipsize: gtk::pango::EllipsizeMode::End,
                #[watch]
                set_label: model.item.label.as_deref().unwrap_or_default(),
                #[watch]
                set_visible: model.item.label.is_some(),
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let action_group = gio::SimpleActionGroup::new();
        root.insert_action_group("exec-status", Some(&action_group));
        let context_menu = gtk::PopoverMenu::from_model(Some(&gio::Menu::new()));
        context_menu.set_parent(&root);
        context_menu.set_has_arrow(false);
        root.connect_destroy({
            let context_menu = context_menu.clone();
            move |_| context_menu.unparent()
        });

        let model = StatusItem {
            item: init.item,
            has_popover: init.has_popover,
            image: gtk::Image::new(),
            context_menu,
            action_group,
        };
        let widgets = view_output!();
        let mut model = model;
        model.image = widgets.image.clone();
        model.apply_icon();
        model.sync_context_menu(&sender);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Click(button) => {
                if button == 3 {
                    if has_visible_menu_items(&self.item.menu) {
                        self.context_menu.popup();
                    } else {
                        let _ = sender.output(Output::ContextMenu);
                    }
                    return;
                }

                let event = self.item.id.as_ref().map(|id| EventPayload {
                    id: id.clone(),
                    kind: EventKind::Click,
                    source: EventSource::Status,
                    button: Some(MouseButton::from_number(button)),
                    active: None,
                    value: None,
                    delta_y: None,
                });

                if self.has_popover && button == 1 {
                    let output = match event {
                        Some(event) => Output::Activate(Some(event)),
                        None => Output::TogglePopover,
                    };
                    let _ = sender.output(output);
                    return;
                }

                if let Some(event) = event {
                    let _ = sender.output(Output::Event(event));
                }
            }
            Input::Scroll(delta_y) => {
                if let Some(id) = &self.item.id {
                    let _ = sender.output(Output::Event(EventPayload {
                        id: id.clone(),
                        kind: EventKind::Scroll,
                        source: EventSource::Status,
                        button: None,
                        active: None,
                        value: None,
                        delta_y: Some(delta_y),
                    }));
                }
            }
            Input::Reconfigure { item, has_popover } => {
                self.item = item;
                self.has_popover = has_popover;
                self.apply_icon();
                self.sync_context_menu(&sender);
            }
        }
    }
}

impl StatusItem {
    fn apply_icon(&self) {
        match &self.item.icon {
            Some(icon) => apply_icon_to_image(&self.image, icon),
            None => self.image.clear(),
        }
    }

    fn sync_context_menu(&self, sender: &ComponentSender<Self>) {
        for action in self.action_group.list_actions() {
            self.action_group.remove_action(action.as_str());
        }

        let menu = gio::Menu::new();
        for (index, item) in self.item.menu.iter().enumerate() {
            if !item.visible {
                continue;
            }

            let action_name = format!("item{index}");
            let action = gio::SimpleAction::new(&action_name, None);
            action.set_enabled(item.enabled);
            action.connect_activate({
                let id = item.id.clone();
                let sender = sender.clone();
                move |_, _| {
                    let _ = sender.output(Output::Event(status_menu_event(id.clone())));
                }
            });
            self.action_group.add_action(&action);
            menu.append(
                Some(&item.label),
                Some(&format!("exec-status.{action_name}")),
            );
        }

        self.context_menu.set_menu_model(Some(&menu));
        if !has_visible_menu_items(&self.item.menu) {
            self.context_menu.popdown();
        }
    }
}

fn has_visible_menu_items(items: &[StatusMenuItem]) -> bool {
    items.iter().any(|item| item.visible)
}

fn status_menu_event(id: String) -> EventPayload {
    EventPayload {
        id,
        kind: EventKind::Click,
        source: EventSource::Status,
        button: Some(MouseButton::Left),
        active: None,
        value: None,
        delta_y: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_menu_items_ignore_hidden_entries() {
        let items = vec![StatusMenuItem {
            id: "hidden".into(),
            label: "Hidden".into(),
            visible: false,
            enabled: true,
        }];

        assert!(!has_visible_menu_items(&items));
    }

    #[test]
    fn status_menu_events_are_status_clicks() {
        assert_eq!(
            status_menu_event("settings".into()),
            EventPayload {
                id: "settings".into(),
                kind: EventKind::Click,
                source: EventSource::Status,
                button: Some(MouseButton::Left),
                active: None,
                value: None,
                delta_y: None,
            }
        );
    }
}
