#![allow(unused_assignments)]

use std::fmt::Debug;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionMenuItem<Command> {
    pub label: String,
    pub icon: Option<String>,
    pub visible: bool,
    pub checked: Option<bool>,
    pub selectable: Option<bool>,
    pub command: Command,
}

pub struct ActionMenu<Command> {
    header: Option<String>,
    items: Vec<ActionMenuItem<Command>>,
    list: gtk::Box,
}

#[derive(Debug)]
pub struct Init<Command> {
    pub header: Option<String>,
    pub items: Vec<ActionMenuItem<Command>>,
}

#[derive(Debug)]
pub enum Input<Command> {
    Update(Vec<ActionMenuItem<Command>>),
}

struct ActionItemRowInit {
    label: String,
    icon: Option<String>,
    visible: bool,
    selectable: bool,
}

#[relm4::widget_template]
impl WidgetTemplate for ActionItemRow {
    type Init = ActionItemRowInit;

    view! {
        gtk::Box {
            add_css_class: "action-row",
            set_visible: init.visible,

            #[name = "button"]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "action-row__button",

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    add_css_class: "action-row__content-shell",

                    gtk::Image {
                        set_icon_name: init.icon.as_deref(),
                        set_pixel_size: 16,
                        set_visible: init.icon.is_some(),
                        add_css_class: "action-row__leading",
                    },

                    gtk::Label {
                        set_label: &init.label,
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                        add_css_class: "action-row__title",
                    },

                    gtk::Image {
                        set_icon_name: Some("object-select-symbolic"),
                        set_pixel_size: 14,
                        set_visible: init.selectable,
                        add_css_class: "action-row__trailing",
                    },
                }
            }
        }
    }
}

#[relm4::component(pub)]
impl<Command> SimpleComponent for ActionMenu<Command>
where
    Command: Clone + Debug + 'static,
{
    type Init = Init<Command>;
    type Input = Input<Command>;
    type Output = Command;

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "action-menu",
            #[watch]
            set_visible: has_visible_items(&model.items),

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "action-menu__header",
                #[watch]
                set_visible: model.header.is_some(),

                gtk::Label {
                    #[watch]
                    set_label: model.header.as_deref().unwrap_or(""),
                    set_halign: gtk::Align::Start,
                    add_css_class: "action-menu__title",
                },
            },

            #[name(list)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "action-menu__body",
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ActionMenu {
            header: init.header,
            items: init.items,
            list: gtk::Box::new(gtk::Orientation::Vertical, 0),
        };

        let widgets = view_output!();
        let mut model = model;
        model.list = widgets.list.clone();
        render_items(&model.list, &model.items, &sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            Input::Update(items) => {
                self.items = items;
                render_items(&self.list, &self.items, &sender);
            }
        }
    }
}

fn has_visible_items<Command>(items: &[ActionMenuItem<Command>]) -> bool {
    items.iter().any(|item| item.visible)
}

fn render_items<Command>(
    list: &gtk::Box,
    items: &[ActionMenuItem<Command>],
    sender: &ComponentSender<ActionMenu<Command>>,
) where
    Command: Clone + Debug + 'static,
{
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    for item in items {
        let selectable = item.selectable.unwrap_or(item.checked.is_some());
        let checked = item.checked.unwrap_or(false);
        let row = ActionItemRow::init(ActionItemRowInit {
            label: item.label.clone(),
            icon: item.icon.clone(),
            visible: item.visible,
            selectable,
        });

        row.as_ref().set_visible(item.visible);
        row.as_ref().remove_css_class("is-checked");
        row.as_ref().remove_css_class("is-selected");
        if checked {
            row.as_ref().add_css_class("is-checked");
            row.as_ref().add_css_class("is-selected");
        }

        let output = item.command.clone();
        let sender = sender.clone();
        row.button.connect_clicked(move |_| {
            let _ = sender.output(output.clone());
        });

        list.append(row.as_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(visible: bool) -> ActionMenuItem<&'static str> {
        ActionMenuItem {
            label: "Action".into(),
            icon: None,
            visible,
            checked: None,
            selectable: None,
            command: "action",
        }
    }

    #[test]
    fn menu_visibility_follows_visible_items() {
        assert!(!has_visible_items::<&str>(&[]));
        assert!(!has_visible_items(&[item(false)]));
        assert!(has_visible_items(&[item(false), item(true)]));
    }
}
