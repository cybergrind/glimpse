use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for PrivacyIndicators {
    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 6,
            set_valign: gtk::Align::Center,

            #[name = "microphone"]
            gtk::Image {
                add_css_class: "privacy-indicator",
                set_icon_name: Some("audio-input-microphone-symbolic"),
                set_pixel_size: 16,
                set_visible: false,
            },

            #[name = "camera"]
            gtk::Image {
                add_css_class: "privacy-indicator",
                set_icon_name: Some("camera-web-symbolic"),
                set_pixel_size: 16,
                set_visible: false,
            },

            #[name = "screen"]
            gtk::Image {
                add_css_class: "privacy-indicator",
                add_css_class: "privacy-screen-indicator",
                set_icon_name: Some("video-display-symbolic"),
                set_pixel_size: 16,
                set_visible: false,
            },

            #[name = "location"]
            gtk::Image {
                add_css_class: "privacy-indicator",
                set_icon_name: Some("find-location-symbolic"),
                set_pixel_size: 16,
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
    fn privacy_indicators_exposes_all_icon_slots() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let indicators = PrivacyIndicators::init(());

        assert_eq!(indicators.spacing(), 6);
        assert!(!indicators.microphone.is_visible());
        assert!(indicators.microphone.has_css_class("privacy-indicator"));
        assert!(!indicators.camera.is_visible());
        assert!(indicators.camera.has_css_class("privacy-indicator"));
        assert!(!indicators.screen.is_visible());
        assert!(indicators.screen.has_css_class("privacy-indicator"));
        assert!(indicators.screen.has_css_class("privacy-screen-indicator"));
        assert!(!indicators.location.is_visible());
        assert!(indicators.location.has_css_class("privacy-indicator"));
    }
}
