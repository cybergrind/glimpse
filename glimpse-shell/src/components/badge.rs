use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for BadgeView {
    view! {
        gtk::Label {
            add_css_class: "badge",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn badge_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let badge = BadgeView::init(());

        assert!(badge.has_css_class("badge"));
    }
}
