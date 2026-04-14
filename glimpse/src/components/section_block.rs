use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Default)]
pub struct SectionBlockInit {
    pub title: String,
    pub subtitle: String,
}

pub struct SectionBlock {
    title: String,
    subtitle: String,
}

#[derive(Debug)]
pub enum SectionBlockInput {
    Update {
        title: String,
        subtitle: String,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for SectionBlock {
    type Init = SectionBlockInit;
    type Input = SectionBlockInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "section-block",

            #[name(header)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                add_css_class: "section-block__header",

                #[name(title)]
                gtk::Label {
                    #[watch]
                    set_label: &model.title,
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                    add_css_class: "section-block__title",
                },

                #[name(subtitle)]
                gtk::Label {
                    #[watch]
                    set_label: &model.subtitle,
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                    add_css_class: "section-block__subtitle",
                    #[watch]
                    set_visible: !model.subtitle.is_empty(),
                },
            },

            #[name(body)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                add_css_class: "section-block__body",
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = SectionBlock {
            title: init.title,
            subtitle: init.subtitle,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let SectionBlockInput::Update { title, subtitle } = message;
        self.title = title;
        self.subtitle = subtitle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::{Component, ComponentController};

    #[test]
    fn section_block_exposes_shared_header_classes() {
        if gtk::init().is_err() {
            return;
        }

        let component = SectionBlock::builder().launch(SectionBlockInit {
            title: "Devices".into(),
            subtitle: "Connected now".into(),
        });
        let root = component.widget();
        let header = root
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("section block should have header");
        let title = header
            .first_child()
            .and_downcast::<gtk::Label>()
            .expect("header should have title");
        let subtitle = title
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("header should have subtitle");
        let body = header
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("section block should have body");

        assert!(root.has_css_class("section-block"));
        assert!(header.has_css_class("section-block__header"));
        assert!(title.has_css_class("section-block__title"));
        assert!(subtitle.has_css_class("section-block__subtitle"));
        assert!(body.has_css_class("section-block__body"));
    }
}
