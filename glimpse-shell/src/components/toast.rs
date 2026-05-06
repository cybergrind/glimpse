use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for ToastView {
    view! {
        gtk::Box {
            add_css_class: "toast",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_valign: gtk::Align::Center,

            #[name = "icon"]
            gtk::Image {
                add_css_class: "toast__icon",
                set_pixel_size: 16,
                set_visible: false,
            },

            gtk::Box {
                add_css_class: "toast__text",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,

                #[name = "title"]
                gtk::Label {
                    add_css_class: "toast__title",
                    set_xalign: 0.0,
                },

                #[name = "message"]
                gtk::Label {
                    add_css_class: "toast__message",
                    set_xalign: 0.0,
                    set_visible: false,
                },
            },

            #[name = "action"]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "toast__action",
                set_visible: false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn toast_view_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let toast = ToastView::init(());

        assert!(toast.has_css_class("toast"));
        assert!(toast.icon.has_css_class("toast__icon"));
        assert!(toast.title.has_css_class("toast__title"));
        assert!(toast.message.has_css_class("toast__message"));
        assert!(toast.action.has_css_class("toast__action"));
    }
}
