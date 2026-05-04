use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for SectionHeader {
    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 2,
            add_css_class: "section-header",
            add_css_class: "section-block__header",

            #[name = "title"]
            gtk::Label {
                add_css_class: "section-header__title",
                add_css_class: "section-block__title",
                set_halign: gtk::Align::Start,
                set_xalign: 0.0,
            },

            #[name = "subtitle"]
            gtk::Label {
                add_css_class: "section-header__subtitle",
                add_css_class: "section-block__subtitle",
                set_halign: gtk::Align::Start,
                set_xalign: 0.0,
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
    fn section_header_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let header = SectionHeader::init(());

        assert!(header.has_css_class("section-header"));
        assert!(header.has_css_class("section-block__header"));
        assert!(header.title.has_css_class("section-header__title"));
        assert!(header.title.has_css_class("section-block__title"));
        assert!(header.subtitle.has_css_class("section-header__subtitle"));
        assert!(header.subtitle.has_css_class("section-block__subtitle"));
        assert!(!header.subtitle.is_visible());
    }
}
