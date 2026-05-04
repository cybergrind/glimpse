use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for CardSurface {
    view! {
        gtk::Box {
            add_css_class: "card-surface",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            #[name = "body"]
            gtk::Box {
                add_css_class: "card-surface__body",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn card_surface_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let card = CardSurface::init(());

        assert!(card.has_css_class("card-surface"));
        assert!(card.body.has_css_class("card-surface__body"));
    }
}
