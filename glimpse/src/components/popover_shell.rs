use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Default)]
pub struct PopoverShellInit {
    pub show_footer: bool,
}

pub struct PopoverShell {
    show_footer: bool,
}

#[derive(Debug)]
pub enum PopoverShellInput {
    SetFooterVisible(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for PopoverShell {
    type Init = PopoverShellInit;
    type Input = PopoverShellInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "popover-shell",

            #[name(content)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "popover-shell__content",
            },

            #[name(footer)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "popover-shell__footer",
                #[watch]
                set_visible: model.show_footer,
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = PopoverShell {
            show_footer: init.show_footer,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let PopoverShellInput::SetFooterVisible(visible) = message;
        self.show_footer = visible;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::{Component, ComponentController};

    #[test]
    fn popover_shell_exposes_stable_class_contract() {
        if gtk::init().is_err() {
            return;
        }

        let component = PopoverShell::builder().launch(PopoverShellInit::default());
        let root = component.widget();
        let content = root
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should have content");
        let footer = content
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("popover shell should have footer");

        assert!(root.has_css_class("popover-shell"));
        assert!(content.has_css_class("popover-shell__content"));
        assert!(footer.has_css_class("popover-shell__footer"));
        assert!(!footer.is_visible());
    }
}
