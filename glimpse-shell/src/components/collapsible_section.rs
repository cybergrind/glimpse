use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for CollapsibleSectionView {
    view! {
        gtk::Box {
            add_css_class: "collapsible-section",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            #[name = "button"]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "action-row__button",

                gtk::Box {
                    add_css_class: "action-row__content-shell",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,

                    #[name = "title"]
                    gtk::Label {
                        add_css_class: "action-row__title",
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                    },

                    #[name = "chevron"]
                    gtk::Image {
                        add_css_class: "collapsible-section__chevron",
                        add_css_class: "action-row__meta",
                        set_pixel_size: 16,
                        set_icon_name: Some("pan-end-symbolic"),
                    },
                },
            },

            #[name = "content"]
            gtk::Box {
                add_css_class: "collapsible-section__content",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
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
    fn collapsible_section_view_exposes_stable_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let section = CollapsibleSectionView::init(());

        assert!(section.has_css_class("collapsible-section"));
        assert!(section.button.has_css_class("action-row__button"));
        assert!(section.title.has_css_class("action-row__title"));
        assert!(
            section
                .chevron
                .has_css_class("collapsible-section__chevron")
        );
        assert!(
            section
                .content
                .has_css_class("collapsible-section__content")
        );
        assert!(!section.content.is_visible());
    }
}
