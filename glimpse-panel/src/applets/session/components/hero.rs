use glimpse::providers::session_actions::SessionSnapshot;
use relm4::{
    gtk::{self, prelude::*},
    ComponentParts, ComponentSender, SimpleComponent,
};

const FALLBACK_USER_NAME: &str = "user";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeroView {
    pub name: String,
    pub subtitle: String,
}

impl Default for SessionHeroView {
    fn default() -> Self {
        Self {
            name: FALLBACK_USER_NAME.into(),
            subtitle: String::new(),
        }
    }
}

impl From<&SessionSnapshot> for SessionHeroView {
    fn from(snapshot: &SessionSnapshot) -> Self {
        Self {
            name: if snapshot.user_name.trim().is_empty() {
                FALLBACK_USER_NAME.into()
            } else {
                snapshot.user_name.clone()
            },
            subtitle: snapshot.subtitle.clone(),
        }
    }
}

pub struct SessionHero {
    view: SessionHeroView,
}

#[derive(Debug)]
pub enum SessionHeroInput {
    Update(SessionHeroView),
}

#[relm4::component(pub)]
impl SimpleComponent for SessionHero {
    type Init = SessionHeroView;
    type Input = SessionHeroInput;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 12,
            add_css_class: "session-hero",

            gtk::Image {
                set_icon_name: Some("avatar-default-symbolic"),
                set_pixel_size: 32,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,
                set_valign: gtk::Align::Center,

                gtk::Label {
                    #[watch]
                    set_label: &model.view.name,
                    set_halign: gtk::Align::Start,
                    add_css_class: "session-hero-name",
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.view.subtitle,
                    set_halign: gtk::Align::Start,
                    add_css_class: "session-hero-subtitle",
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = SessionHero { view: init };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let SessionHeroInput::Update(view) = message;
        self.view = view;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_view_uses_fallback_user_name() {
        let view = SessionHeroView::default();

        assert_eq!(view.name, "user");
        assert!(view.subtitle.is_empty());
    }

    #[test]
    fn snapshot_conversion_uses_fallback_when_user_name_is_missing() {
        let snapshot = SessionSnapshot::default();

        let view = SessionHeroView::from(&snapshot);

        assert_eq!(view.name, "user");
    }
}
