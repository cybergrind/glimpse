use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for DeviceStatusView {
    view! {
        gtk::Box {
            add_css_class: "device-status",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 6,
            set_halign: gtk::Align::End,
            set_valign: gtk::Align::Center,

            #[name = "spinner"]
            gtk::Spinner {
                add_css_class: "device-status__spinner",
                set_visible: false,
                set_spinning: false,
            },

            #[name = "label"]
            gtk::Label {
                add_css_class: "device-status__label",
                add_css_class: "action-row__meta",
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Center,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn device_status_view_exposes_label_and_spinner() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let status = DeviceStatusView::init(());

        assert!(status.has_css_class("device-status"));
        assert!(status.spinner.has_css_class("device-status__spinner"));
        assert!(status.label.has_css_class("device-status__label"));
        assert!(status.label.has_css_class("action-row__meta"));
    }
}
