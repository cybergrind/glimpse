use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for StatusDotView {
    view! {
        gtk::Box {
            add_css_class: "status-dot",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn status_dot_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let dot = StatusDotView::init(());

        assert!(dot.has_css_class("status-dot"));
    }
}
