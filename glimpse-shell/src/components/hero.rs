#![allow(dead_code)]

use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for HeroView {
    view! {
        gtk::Box {
            add_css_class: "hero-row",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,

            #[name = "icon"]
            gtk::Image {
                add_css_class: "hero-row__media",
                set_pixel_size: 32,
                set_icon_name: Some("image-missing-symbolic"),
            },

            gtk::Box {
                add_css_class: "hero-row__content",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                #[name = "title"]
                gtk::Label {
                    add_css_class: "hero-row__title",
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                },

                #[name = "subtitle"]
                gtk::Label {
                    add_css_class: "hero-row__subtitle",
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                },
            },

            #[name = "trailing"]
            gtk::Box {
                add_css_class: "hero-row__trailing",
                set_orientation: gtk::Orientation::Horizontal,
                set_valign: gtk::Align::Center,
                set_visible: false,

                #[name = "toggle"]
                gtk::Switch {
                    add_css_class: "hero-row__toggle",
                    set_valign: gtk::Align::Center,
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hero_view_exposes_shared_class_contract() {
        if gtk::init().is_err() {
            return;
        }

        let hero = HeroView::init(());

        assert!(hero.has_css_class("hero-row"));
        assert!(hero.icon.has_css_class("hero-row__media"));
        assert!(hero.title.has_css_class("hero-row__title"));
        assert!(hero.subtitle.has_css_class("hero-row__subtitle"));
        assert!(hero.trailing.has_css_class("hero-row__trailing"));
        assert!(hero.toggle.has_css_class("hero-row__toggle"));
        assert!(!hero.trailing.is_visible());
    }
}
