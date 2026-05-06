use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for CopyableView {
    view! {
        gtk::Box {
            add_css_class: "copyable",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_valign: gtk::Align::Center,

            #[name = "label"]
            gtk::Label {
                add_css_class: "copyable__label",
                set_xalign: 0.0,
                set_visible: false,
            },

            #[name = "value"]
            gtk::Label {
                add_css_class: "copyable__value",
                set_xalign: 0.0,
                set_hexpand: true,
                set_selectable: true,
            },

            #[name = "button"]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "copyable__button",
                set_tooltip_text: Some("Copy"),

                gtk::Image {
                    set_icon_name: Some("edit-copy-symbolic"),
                    set_pixel_size: 16,
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn copyable_view_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let copyable = CopyableView::init(());

        assert!(copyable.has_css_class("copyable"));
        assert!(copyable.label.has_css_class("copyable__label"));
        assert!(copyable.value.has_css_class("copyable__value"));
        assert!(copyable.button.has_css_class("copyable__button"));
    }
}
