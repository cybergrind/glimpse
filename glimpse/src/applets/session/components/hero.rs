use glimpse::session_actions::provider::SessionSnapshot;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::components::hero_row::{HeroRow, HeroRowInit, HeroRowInput};

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
    row: Controller<HeroRow>,
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
            #[local_ref]
            row_widget -> gtk::Box {}
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let row = HeroRow::builder()
            .launch(HeroRowInit {
                title: init.name,
                subtitle: init.subtitle,
            })
            .detach();
        let row_widget = row.widget().clone();
        let media = row_widget
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("hero row should expose media box");
        media.append(&gtk::Image::from_icon_name("avatar-default-symbolic"));

        let model = SessionHero { row };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let SessionHeroInput::Update(view) = message;
        self.row.emit(HeroRowInput::Update {
            title: view.name,
            subtitle: view.subtitle,
        });
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
