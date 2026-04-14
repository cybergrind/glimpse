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
use crate::components::{
    footer_action::{FooterAction, FooterActionInit},
    popover_shell::{PopoverShell, PopoverShellInit},
};

pub struct NotificationsPopover {
    popover: gtk::Popover,
    shell: Controller<PopoverShell>,
    hero: Controller<NotificationsHero>,
    list: Controller<NotificationsList>,
    footer: Controller<FooterAction>,
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
            #[name(shell_slot)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: false,
                set_overflow: gtk::Overflow::Hidden,

                #[name(shell_content_slot)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },

                #[name(shell_footer_slot)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
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

        let shell = PopoverShell::builder()
            .launch(PopoverShellInit { show_footer: true })
            .detach();
        widgets.shell_slot.append(shell.widget());

        let hero = NotificationsHero::builder()
            .launch(NotificationsHeroInit {
                emit_command: init.emit_command.clone(),
            })
            .detach();

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

        let footer = FooterAction::builder()
            .launch(FooterActionInit {
                title: "Clear All".into(),
                subtitle: String::new(),
            })
            .detach();

        let shell_root = shell.widget().clone();
        let shell_content = shell_root
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should expose content");
        let shell_footer = shell_content
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should expose footer");

        shell_content.append(hero.widget());
        shell_content.append(list.widget());
        shell_footer.append(footer.widget());

        if let Some(button) = footer
            .widget()
            .first_child()
            .and_downcast::<gtk::Box>()
            .and_then(|row| row.first_child().and_downcast::<gtk::Button>())
        {
            button.connect_clicked({
                let sender = sender.clone();
                move |_| {
                    sender.input(NotificationsPopoverInput::ClearAll);
                }
            });
        }

        let model = NotificationsPopover {
            popover: widgets.root.clone(),
            shell,
            hero,
            list,
            footer,
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

#[cfg(test)]
mod tests {
    const POPOVER_SOURCE: &str = include_str!("popover.rs");

    #[test]
    fn notifications_popover_uses_shared_shell_and_footer() {
        assert!(POPOVER_SOURCE.contains("PopoverShell::builder()"));
        assert!(POPOVER_SOURCE.contains("FooterAction::builder()"));
        assert!(POPOVER_SOURCE.contains("shell_content.append(hero.widget())"));
        assert!(POPOVER_SOURCE.contains("shell_content.append(list.widget())"));
        assert!(POPOVER_SOURCE.contains("shell_footer.append(footer.widget())"));
    }
}
