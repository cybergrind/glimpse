use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

pub(crate) struct NotificationActionButtonInit {
    pub label: String,
    pub style: NotificationActionButtonStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationActionButtonStyle {
    Popover,
    Popup,
}

impl NotificationActionButtonStyle {
    fn class_name(self) -> &'static str {
        match self {
            Self::Popover => "notification-action-button",
            Self::Popup => "popup-action-btn",
        }
    }
}

#[relm4::widget_template(pub(crate))]
impl WidgetTemplate for NotificationActionButton {
    type Init = NotificationActionButtonInit;

    view! {
        gtk::Button {
            add_css_class: "flat",
            add_css_class: init.style.class_name(),
            set_label: &init.label,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn action_button_template_applies_style_class() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let button = NotificationActionButton::init(NotificationActionButtonInit {
            label: "Open".into(),
            style: NotificationActionButtonStyle::Popup,
        });

        assert!(button.as_ref().has_css_class("flat"));
        assert!(button.as_ref().has_css_class("popup-action-btn"));
    }
}
