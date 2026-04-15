use std::collections::HashMap;
use std::rc::Rc;

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::NotificationActionCommand;
use super::components::{
    NotifData, NotificationCommandEmitter, StackToggleEmitter,
    hero::{NotificationsHero, NotificationsHeroInit, NotificationsHeroInput},
    list::{NotificationsList, NotificationsListInit, NotificationsListInput},
};

pub struct NotificationsPopover {
    popover: gtk::Popover,
    hero: Controller<NotificationsHero>,
    list: Controller<NotificationsList>,
    emit_command: NotificationCommandEmitter,
    dnd: bool,
    count: u32,
    stack_state: HashMap<String, bool>,
    last_notifications: Vec<NotifData>,
}

pub struct NotificationsPopoverInit {
    pub parent: gtk::Box,
    pub emit_command: Rc<dyn Fn(NotificationActionCommand)>,
}

#[derive(Debug)]
pub enum NotificationsPopoverInput {
    Toggle,
    UpdateStatus { dnd: bool, count: u32 },
    UpdateList(Vec<NotifData>),
    ToggleStack(String),
    ClearAll,
}

#[allow(unused_assignments)]
#[relm4::component(pub)]
impl SimpleComponent for NotificationsPopover {
    type Init = NotificationsPopoverInit;
    type Input = NotificationsPopoverInput;
    type Output = ();

    view! {
        root = gtk::Popover {
            #[name(body)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: false,
                set_overflow: gtk::Overflow::Hidden,

                #[name(hero_slot)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[name(list_slot)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "settings-btn",
                    connect_clicked[sender] => move |_| {
                        sender.input(NotificationsPopoverInput::ClearAll);
                    },

                    gtk::Label {
                        set_label: "Clear All",
                        set_halign: gtk::Align::Start,
                    }
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
        widgets.root.add_css_class("notifications-popover");

        let hero = NotificationsHero::builder()
            .launch(NotificationsHeroInit {
                emit_command: init.emit_command.clone(),
            })
            .detach();
        widgets.hero_slot.append(hero.widget());

        let on_toggle_stack: StackToggleEmitter = Rc::new({
            let sender = sender.clone();
            move |app_name| sender.input(NotificationsPopoverInput::ToggleStack(app_name))
        });
        let list = NotificationsList::builder()
            .launch(NotificationsListInit {
                emit_command: init.emit_command.clone(),
                on_toggle_stack,
            })
            .detach();
        widgets.list_slot.append(list.widget());

        let model = NotificationsPopover {
            popover: widgets.root.clone(),
            hero,
            list,
            emit_command: init.emit_command,
            dnd: false,
            count: 0,
            stack_state: HashMap::new(),
            last_notifications: Vec::new(),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NotificationsPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            NotificationsPopoverInput::UpdateStatus { dnd, count } => {
                self.dnd = dnd;
                self.count = count;
                self.hero
                    .emit(NotificationsHeroInput::UpdateStatus { dnd, count });
            }
            NotificationsPopoverInput::UpdateList(data) => {
                self.last_notifications = data;
                self.rebuild_list(&sender);
            }
            NotificationsPopoverInput::ToggleStack(app_name) => {
                let current = *self.stack_state.get(&app_name).unwrap_or(&true);
                self.stack_state.insert(app_name, !current);
                self.rebuild_list(&sender);
            }
            NotificationsPopoverInput::ClearAll => {
                (self.emit_command)(NotificationActionCommand::DismissAll);
            }
        }
    }
}

impl NotificationsPopover {
    fn rebuild_list(&self, sender: &ComponentSender<Self>) {
        let _ = sender;
        self.list.emit(NotificationsListInput::Sync {
            notifications: self.last_notifications.clone(),
            stack_state: self.stack_state.clone(),
        });
    }
}
