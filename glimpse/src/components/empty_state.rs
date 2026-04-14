use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Default)]
pub struct EmptyStateInit {
    pub title: String,
    pub subtitle: String,
}

pub struct EmptyState {
    title: String,
    subtitle: String,
}

#[derive(Debug)]
pub enum EmptyStateInput {
    Update {
        title: String,
        subtitle: String,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for EmptyState {
    type Init = EmptyStateInit;
    type Input = EmptyStateInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 4,
            set_halign: gtk::Align::Center,
            add_css_class: "empty-state",

            #[name(title)]
            gtk::Label {
                #[watch]
                set_label: &model.title,
                set_halign: gtk::Align::Center,
                set_xalign: 0.5,
                set_justify: gtk::Justification::Center,
                add_css_class: "empty-state__title",
            },

            #[name(subtitle)]
            gtk::Label {
                #[watch]
                set_label: &model.subtitle,
                set_halign: gtk::Align::Center,
                set_xalign: 0.5,
                set_justify: gtk::Justification::Center,
                add_css_class: "empty-state__subtitle",
                #[watch]
                set_visible: !model.subtitle.is_empty(),
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = EmptyState {
            title: init.title,
            subtitle: init.subtitle,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let EmptyStateInput::Update { title, subtitle } = message;
        self.title = title;
        self.subtitle = subtitle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::{Component, ComponentController};

    #[test]
    fn empty_state_uses_shared_empty_state_classes() {
        if gtk::init().is_err() {
            return;
        }

        let component = EmptyState::builder().launch(EmptyStateInit {
            title: "No notifications".into(),
            subtitle: "You're caught up.".into(),
        });
        let root = component.widget();
        let title = root
            .first_child()
            .and_downcast::<gtk::Label>()
            .expect("empty state should have title");
        let subtitle = title
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("empty state should have subtitle");

        assert!(root.has_css_class("empty-state"));
        assert!(title.has_css_class("empty-state__title"));
        assert!(subtitle.has_css_class("empty-state__subtitle"));
        assert_eq!(title.halign(), gtk::Align::Center);
        assert_eq!(subtitle.halign(), gtk::Align::Center);
        assert_eq!(title.xalign(), 0.5);
        assert_eq!(subtitle.xalign(), 0.5);
    }
}
