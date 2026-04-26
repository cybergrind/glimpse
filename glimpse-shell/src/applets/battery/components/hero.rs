use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::{applets::battery::format, services::battery::BatteryStatus};

#[relm4::widget_template(pub)]
impl WidgetTemplate for BatteryHeroView {
    view! {
        gtk::Box {
            add_css_class: "battery-hero",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            gtk::Box {
                add_css_class: "battery-hero-header",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                #[name = "icon"]
                gtk::Image {
                    set_pixel_size: 32,
                    set_icon_name: Some("battery-missing-symbolic"),
                },

                #[name = "percentage"]
                gtk::Label {
                    add_css_class: "battery-pct",
                    set_label: "\u{2014}",
                },
            },

            #[name = "progress"]
            gtk::ProgressBar {
                add_css_class: "battery-progress",
                set_fraction: 0.0,
            },

            #[name = "state"]
            gtk::Label {
                add_css_class: "battery-state-text",
                set_halign: gtk::Align::Start,
            },
        }
    }
}

impl BatteryHeroView {
    pub fn update_status(&self, status: &BatteryStatus) {
        self.icon.set_icon_name(Some(&status.icon_name));
        self.percentage
            .set_label(&format::percent(status.percentage));
        self.progress.set_fraction(status.percentage as f64 / 100.0);
        self.state.set_label(&format::state_text(status));
    }
}
