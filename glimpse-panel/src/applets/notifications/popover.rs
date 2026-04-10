use std::collections::HashMap;
use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::NotificationActionCommand;
use super::components::{
    NotifData, NotificationCommandEmitter, StackToggleEmitter,
    hero::NotificationsHero,
    list::NotificationsList,
};

pub struct NotificationsPopover {
    popover: gtk::Popover,
    hero: NotificationsHero,
    list: NotificationsList,
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
    UpdateStatus {
        dnd: bool,
        count: u32,
        badge_count: u32,
    },
    UpdateList(Vec<NotifData>),
    ToggleStack(String),
}

impl SimpleComponent for NotificationsPopover {
    type Init = NotificationsPopoverInit;
    type Input = NotificationsPopoverInput;
    type Output = ();
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Popover::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("notifications-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        let hero = NotificationsHero::new(init.emit_command.clone());
        vbox.append(hero.widget());
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let list = NotificationsList::new();
        vbox.append(list.widget());

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let clear_lbl = gtk::Label::new(Some("Clear All"));
        clear_lbl.set_halign(gtk::Align::Start);
        let clear_btn = gtk::Button::new();
        clear_btn.set_child(Some(&clear_lbl));
        clear_btn.add_css_class("flat");
        clear_btn.add_css_class("settings-btn");
        let emit_command = init.emit_command.clone();
        clear_btn.connect_clicked(move |_| {
            emit_command(NotificationActionCommand::DismissAll);
        });
        vbox.append(&clear_btn);

        root.set_child(Some(&vbox));

        let model = NotificationsPopover {
            popover: root.clone(),
            hero,
            list,
            emit_command: init.emit_command,
            dnd: false,
            count: 0,
            stack_state: HashMap::new(),
            last_notifications: Vec::new(),
        };

        ComponentParts { model, widgets: () }
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
            NotificationsPopoverInput::UpdateStatus {
                dnd,
                count,
                badge_count: _,
            } => {
                self.dnd = dnd;
                self.count = count;
                self.hero.update_status(dnd, count);
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
        }
    }
}

impl NotificationsPopover {
    fn rebuild_list(&self, sender: &ComponentSender<Self>) {
        let on_toggle_stack: StackToggleEmitter = Rc::new({
            let sender = sender.clone();
            move |app_name| sender.input(NotificationsPopoverInput::ToggleStack(app_name))
        });
        self.list.rebuild(
            &self.last_notifications,
            &self.stack_state,
            self.emit_command.clone(),
            on_toggle_stack,
        );
    }
}
