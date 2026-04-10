use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::row::{resolve_notif_icon_name, time_ago};
use super::{NotifData, NotificationCommandEmitter, StackToggleEmitter};
use crate::applets::notifications::NotificationActionCommand;

pub struct GroupHeader {
    emit_command: NotificationCommandEmitter,
    on_toggle_stack: StackToggleEmitter,
    app_name: String,
    newest: NotifData,
    stacked: bool,
    dismiss_ids: Vec<u32>,
}

pub struct GroupHeaderInit {
    pub app_name: String,
    pub newest: NotifData,
    pub stacked: bool,
    pub dismiss_ids: Vec<u32>,
    pub emit_command: NotificationCommandEmitter,
    pub on_toggle_stack: StackToggleEmitter,
}

#[derive(Debug)]
pub enum GroupHeaderInput {
    Update {
        app_name: String,
        newest: NotifData,
        stacked: bool,
        dismiss_ids: Vec<u32>,
    },
    Toggle,
    DismissGroup,
}

#[relm4::component(pub)]
impl SimpleComponent for GroupHeader {
    type Init = GroupHeaderInit;
    type Input = GroupHeaderInput;
    type Output = ();

    view! {
        gtk::Box {
            set_spacing: 8,
            add_css_class: "notif-group-header",

            gtk::Image {
                #[watch]
                set_icon_name: Some(&resolve_notif_icon_name(&model.newest)),
                add_css_class: "notif-icon",
                add_css_class: "notif-header-icon",
            },

            gtk::Label {
                #[watch]
                set_label: &model.app_name,
                set_xalign: 0.0,
                add_css_class: "notif-app-name",
                add_css_class: "notif-group-title",
            },

            gtk::Label {
                #[watch]
                set_label: &time_ago(model.newest.timestamp),
                set_xalign: 0.0,
                add_css_class: "notif-time",
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_hexpand: true,
            },

            gtk::Button {
                add_css_class: "flat",
                add_css_class: "notif-expand-btn",
                add_css_class: "notif-dismiss",
                #[watch]
                set_icon_name: if model.stacked {
                    "go-down-symbolic"
                } else {
                    "go-up-symbolic"
                },
                connect_clicked[sender] => move |_| {
                    sender.input(GroupHeaderInput::Toggle);
                },
            },

            gtk::Button {
                add_css_class: "flat",
                add_css_class: "notif-dismiss",
                set_icon_name: "window-close-symbolic",
                set_valign: gtk::Align::Center,
                set_tooltip_text: Some("Dismiss group"),
                connect_clicked[sender] => move |_| {
                    sender.input(GroupHeaderInput::DismissGroup);
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = GroupHeader {
            emit_command: init.emit_command,
            on_toggle_stack: init.on_toggle_stack,
            app_name: init.app_name,
            newest: init.newest,
            stacked: init.stacked,
            dismiss_ids: init.dismiss_ids,
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            GroupHeaderInput::Update {
                app_name,
                newest,
                stacked,
                dismiss_ids,
            } => {
                self.app_name = app_name;
                self.newest = newest;
                self.stacked = stacked;
                self.dismiss_ids = dismiss_ids;
            }
            GroupHeaderInput::Toggle => {
                (self.on_toggle_stack)(self.app_name.clone());
            }
            GroupHeaderInput::DismissGroup => {
                for id in &self.dismiss_ids {
                    (self.emit_command)(NotificationActionCommand::Dismiss { id: *id });
                }
            }
        }
    }
}
