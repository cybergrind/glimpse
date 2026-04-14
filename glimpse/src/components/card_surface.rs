use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Default)]
pub struct CardSurfaceInit {
    pub show_header: bool,
    pub show_footer: bool,
}

pub struct CardSurface {
    show_header: bool,
    show_footer: bool,
}

#[derive(Debug)]
pub enum CardSurfaceInput {
    SetHeaderVisible(bool),
    SetFooterVisible(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for CardSurface {
    type Init = CardSurfaceInit;
    type Input = CardSurfaceInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "card-surface",

            #[name(header)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "card-surface__header",
                #[watch]
                set_visible: model.show_header,
            },

            #[name(body)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "card-surface__body",
            },

            #[name(footer)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "card-surface__footer",
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
        let model = CardSurface {
            show_header: init.show_header,
            show_footer: init.show_footer,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            CardSurfaceInput::SetHeaderVisible(visible) => {
                self.show_header = visible;
            }
            CardSurfaceInput::SetFooterVisible(visible) => {
                self.show_footer = visible;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::{Component, ComponentController};

    #[test]
    fn card_surface_uses_shared_surface_classes() {
        if gtk::init().is_err() {
            return;
        }

        let component = CardSurface::builder().launch(CardSurfaceInit::default());
        let root = component.widget();
        let header = root
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("card should have header");
        let body = header
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("card should have body");
        let footer = body
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("card should have footer");

        assert!(root.has_css_class("card-surface"));
        assert!(header.has_css_class("card-surface__header"));
        assert!(body.has_css_class("card-surface__body"));
        assert!(footer.has_css_class("card-surface__footer"));
        assert!(!header.is_visible());
        assert!(!footer.is_visible());
    }
}
