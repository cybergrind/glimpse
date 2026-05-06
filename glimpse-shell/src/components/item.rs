use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for ItemView {
    view! {
        #[name = "button"]
        gtk::Button {
            add_css_class: "flat",
            add_css_class: "item",
            add_css_class: "item__button",

            #[name = "content"]
            gtk::Box {
                add_css_class: "item__content",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_valign: gtk::Align::Center,

                #[name = "left"]
                gtk::Box {
                    add_css_class: "item__left",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 0,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Center,
                    set_hexpand: false,
                    set_visible: false,
                },

                gtk::Box {
                    add_css_class: "item__text",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 0,
                    set_hexpand: true,

                    #[name = "label"]
                    gtk::Label {
                        add_css_class: "item__label",
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                    },
                },

                #[name = "right"]
                gtk::Box {
                    add_css_class: "item__right",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 0,
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Center,
                    set_hexpand: false,
                    set_visible: false,
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
    fn item_view_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let item = ItemView::init(());

        assert!(item.button.has_css_class("item"));
        assert!(item.button.has_css_class("item__button"));
        assert!(item.content.has_css_class("item__content"));
        assert!(item.left.has_css_class("item__left"));
        assert!(item.label.has_css_class("item__label"));
        assert!(item.right.has_css_class("item__right"));
    }
}
