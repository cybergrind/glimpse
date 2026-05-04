use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for EmptyStateView {
    view! {
        gtk::Box {
            add_css_class: "empty-state",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 4,
            set_halign: gtk::Align::Center,

            #[name = "title"]
            gtk::Label {
                add_css_class: "empty-state__title",
                set_xalign: 0.5,
            },

            #[name = "subtitle"]
            gtk::Label {
                add_css_class: "empty-state__subtitle",
                set_xalign: 0.5,
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
    fn empty_state_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let empty = EmptyStateView::init(());

        assert!(empty.has_css_class("empty-state"));
        assert!(empty.title.has_css_class("empty-state__title"));
        assert!(empty.subtitle.has_css_class("empty-state__subtitle"));
        assert!(!empty.subtitle.is_visible());
    }
}
