use std::cell::Cell;
use std::rc::Rc;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use super::NotificationCommandEmitter;
use crate::applets::notifications::NotificationActionCommand;

pub struct NotificationsHero {
    emit_command: NotificationCommandEmitter,
    updating_dnd: Rc<Cell<bool>>,
    dnd: bool,
    count: u32,
}

pub struct NotificationsHeroInit {
    pub emit_command: NotificationCommandEmitter,
}

#[derive(Debug)]
pub enum NotificationsHeroInput {
    UpdateStatus { dnd: bool, count: u32 },
    ToggleDnd(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for NotificationsHero {
    type Init = NotificationsHeroInit;
    type Input = NotificationsHeroInput;
    type Output = ();

    view! {
        gtk::Box {
            set_spacing: 12,
            add_css_class: "notif-hero",
            add_css_class: "hero-row",

            gtk::Image {
                #[watch]
                set_icon_name: Some(if model.dnd {
                    "notifications-disabled-symbolic"
                } else {
                    "preferences-system-notifications-symbolic"
                }),
                add_css_class: "notif-hero-icon",
                add_css_class: "hero-row__media",
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,
                set_valign: gtk::Align::Center,
                add_css_class: "hero-row__content",

                gtk::Label {
                    set_label: "Notifications",
                    set_halign: gtk::Align::Start,
                    add_css_class: "notif-title",
                    add_css_class: "hero-row__title",
                },

                gtk::Label {
                    #[watch]
                    set_label: &hero_subtitle(model.count),
                    set_halign: gtk::Align::Start,
                    add_css_class: "notif-subtitle",
                    add_css_class: "hero-row__subtitle",
                },
            },

            gtk::Switch {
                #[watch]
                set_active: !model.dnd,
                set_valign: gtk::Align::Center,
                set_tooltip_text: Some("Notifications"),
                add_css_class: "hero-row__trailing",
                connect_state_set[sender, updating_dnd = model.updating_dnd.clone()] => move |_, active| {
                    if updating_dnd.get() {
                        return glib::Propagation::Stop;
                    }
                    sender.input(NotificationsHeroInput::ToggleDnd(active));
                    glib::Propagation::Stop
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = NotificationsHero {
            emit_command: init.emit_command,
            updating_dnd: Rc::new(Cell::new(false)),
            dnd: false,
            count: 0,
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NotificationsHeroInput::UpdateStatus { dnd, count } => {
                self.dnd = dnd;
                self.count = count;
            }
            NotificationsHeroInput::ToggleDnd(active) => {
                (self.emit_command)(NotificationActionCommand::SetDnd(!active));
            }
        }
    }
}

fn hero_subtitle(count: u32) -> String {
    if count == 0 {
        "No notifications".to_string()
    } else if count == 1 {
        "1 notification".to_string()
    } else {
        format!("{count} notifications")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::{Component, ComponentController};
    use std::rc::Rc;

    #[test]
    fn notifications_hero_uses_shared_hero_row_classes() {
        if gtk::init().is_err() {
            return;
        }

        let component = NotificationsHero::builder().launch(NotificationsHeroInit {
            emit_command: Rc::new(|_| {}),
        });
        let root = component.widget();
        let media = root
            .first_child()
            .and_downcast::<gtk::Image>()
            .expect("hero should have media");
        let content = media
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("hero should have content");
        let title = content
            .first_child()
            .and_downcast::<gtk::Label>()
            .expect("hero content should have title");
        let subtitle = title
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("hero content should have subtitle");
        let trailing = content
            .next_sibling()
            .and_downcast::<gtk::Switch>()
            .expect("hero should have trailing switch");

        assert!(root.has_css_class("hero-row"));
        assert!(media.has_css_class("hero-row__media"));
        assert!(content.has_css_class("hero-row__content"));
        assert!(title.has_css_class("hero-row__title"));
        assert!(subtitle.has_css_class("hero-row__subtitle"));
        assert!(trailing.has_css_class("hero-row__trailing"));
    }
}
