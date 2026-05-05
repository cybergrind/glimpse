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
            gtk::Box {
                add_css_class: "privacy-recording-pill",
                add_css_class: "privacy-screen-indicator",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 4,
                set_valign: gtk::Align::Center,
                set_visible: false,

                gtk::Image {
                    add_css_class: "privacy-recording-icon",
                    add_css_class: "privacy-screen-indicator",
                    set_icon_name: Some("media-record-symbolic"),
                    set_pixel_size: 16,
                },

                #[name = "screen_elapsed"]
                gtk::Label {
                    add_css_class: "privacy-recording-label",
                    add_css_class: "privacy-screen-indicator",
                    set_label: "0:00",
                    set_valign: gtk::Align::Center,
                },
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
        assert!(indicators.screen.has_css_class("privacy-recording-pill"));
        assert!(indicators.screen.has_css_class("privacy-screen-indicator"));
        assert_eq!(indicators.screen.spacing(), 4);
        assert_eq!(indicators.screen_elapsed.label(), "0:00");
        assert!(!indicators.location.is_visible());
        assert!(indicators.location.has_css_class("privacy-indicator"));
    }
}
