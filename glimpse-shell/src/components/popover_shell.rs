use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[relm4::widget_template(pub)]
impl WidgetTemplate for PopoverShell {
    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "popover-shell",

            #[name = "content"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "popover-shell__content",
            },

            #[name = "footer"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "popover-shell__footer",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn popover_shell_template_exposes_stable_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let shell = PopoverShell::init(());

        assert!(shell.has_css_class("popover-shell"));
        assert!(shell.content.has_css_class("popover-shell__content"));
        assert!(shell.footer.has_css_class("popover-shell__footer"));
    }
}
