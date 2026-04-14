use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::action_row::{ActionRow, ActionRowInit, ActionRowInput, RowVariant};

#[derive(Debug, Clone, Default)]
pub struct FooterActionInit {
    pub title: String,
    pub subtitle: String,
}

pub struct FooterAction {
    row: Controller<ActionRow>,
}

#[derive(Debug)]
pub enum FooterActionInput {
    Update {
        title: String,
        subtitle: String,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for FooterAction {
    type Init = FooterActionInit;
    type Input = FooterActionInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let row = ActionRow::builder()
            .launch(ActionRowInit {
                title: init.title,
                subtitle: init.subtitle,
                variant: RowVariant::Footer,
            })
            .detach();
        root.append(row.widget());

        let model = FooterAction { row };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let FooterActionInput::Update { title, subtitle } = message;
        self.row.emit(ActionRowInput::Update { title, subtitle });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::{Component, ComponentController};

    #[test]
    fn footer_action_wraps_action_row_footer_variant() {
        if gtk::init().is_err() {
            return;
        }

        let component = FooterAction::builder().launch(FooterActionInit {
            title: "Settings".into(),
            subtitle: String::new(),
        });
        let root = component.widget();
        let row = root
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("footer action should contain row root");

        assert!(row.has_css_class("footer-action"));
        assert!(row.has_css_class("action-row"));
        assert!(row.has_css_class("action-row--footer"));
    }
}
