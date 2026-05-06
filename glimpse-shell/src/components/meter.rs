use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for MeterView {
    view! {
        gtk::Box {
            add_css_class: "meter",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 4,

            gtk::Box {
                add_css_class: "meter__header",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_valign: gtk::Align::Center,

                #[name = "icon"]
                gtk::Image {
                    add_css_class: "meter__icon",
                    set_pixel_size: 16,
                    set_visible: false,
                },

                #[name = "label"]
                gtk::Label {
                    add_css_class: "meter__label",
                    set_xalign: 0.0,
                    set_hexpand: true,
                },

                #[name = "value"]
                gtk::Label {
                    add_css_class: "meter__value",
                    set_visible: false,
                },
            },

            #[name = "control"]
            gtk::Box {
                add_css_class: "meter__control",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn meter_view_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let meter = MeterView::init(());

        assert!(meter.has_css_class("meter"));
        assert!(meter.icon.has_css_class("meter__icon"));
        assert!(meter.label.has_css_class("meter__label"));
        assert!(meter.value.has_css_class("meter__value"));
        assert!(meter.control.has_css_class("meter__control"));
    }
}
