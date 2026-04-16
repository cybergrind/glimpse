use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Default)]
pub struct HeroRowInit {
    pub title: String,
    pub subtitle: String,
}

pub struct HeroRow {
    title: String,
    subtitle: String,
}

#[derive(Debug)]
pub enum HeroRowInput {
    Update { title: String, subtitle: String },
}

#[relm4::component(pub)]
impl SimpleComponent for HeroRow {
    type Init = HeroRowInit;
    type Input = HeroRowInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 12,
            add_css_class: "hero-row",

            #[name(media)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 0,
                set_valign: gtk::Align::Center,
                add_css_class: "hero-row__media",
            },

            #[name(content)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,
                set_valign: gtk::Align::Center,
                add_css_class: "hero-row__content",

                #[name(title)]
                gtk::Label {
                    #[watch]
                    set_label: &model.title,
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                    add_css_class: "hero-row__title",
                },

                #[name(subtitle)]
                gtk::Label {
                    #[watch]
                    set_label: &model.subtitle,
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                    add_css_class: "hero-row__subtitle",
                    #[watch]
                    set_visible: !model.subtitle.is_empty(),
                },
            },

            #[name(trailing)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 0,
                set_valign: gtk::Align::Center,
                add_css_class: "hero-row__trailing",
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = HeroRow {
            title: init.title,
            subtitle: init.subtitle,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let HeroRowInput::Update { title, subtitle } = message;
        self.title = title;
        self.subtitle = subtitle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::{Component, ComponentController};

    #[test]
    fn hero_row_exposes_shared_slot_classes() {
        if gtk::init().is_err() {
            return;
        }

        let component = HeroRow::builder().launch(HeroRowInit {
            title: "Network".into(),
            subtitle: "Offline".into(),
        });
        let root = component.widget();
        let media = root
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("hero row should have media");
        let content = media
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("hero row should have content");
        let title = content
            .first_child()
            .and_downcast::<gtk::Label>()
            .expect("hero content should have title");
        let subtitle = title
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("hero content should have subtitle");
        let trailing = content
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("hero row should have trailing");

        assert!(root.has_css_class("hero-row"));
        assert!(media.has_css_class("hero-row__media"));
        assert!(content.has_css_class("hero-row__content"));
        assert!(title.has_css_class("hero-row__title"));
        assert!(subtitle.has_css_class("hero-row__subtitle"));
        assert!(trailing.has_css_class("hero-row__trailing"));
    }
}
