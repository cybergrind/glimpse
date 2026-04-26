use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for DegradedWarningView {
    view! {
        gtk::Box {
            add_css_class: "profile-degraded-row",
            add_css_class: "is-warning",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 6,
            set_visible: false,

            gtk::Image {
                set_icon_name: Some("dialog-warning-symbolic"),
                set_pixel_size: 14,
            },

            #[name = "label"]
            gtk::Label {
                add_css_class: "profile-degraded",
                set_halign: gtk::Align::Start,
                set_wrap: true,
            },
        }
    }
}
