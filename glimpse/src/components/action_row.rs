use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowVariant {
    #[default]
    Default,
    Footer,
}

#[derive(Debug, Clone, Default)]
pub struct ActionRowInit {
    pub title: String,
    pub subtitle: String,
    pub variant: RowVariant,
}

pub struct ActionRow {
    title: String,
    subtitle: String,
    variant: RowVariant,
}

#[derive(Debug)]
pub enum ActionRowInput {
    Update {
        title: String,
        subtitle: String,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for ActionRow {
    type Init = ActionRowInit;
    type Input = ActionRowInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 0,
            add_css_class: "action-row",
            #[watch]
            set_css_classes: &root_classes(model.variant),

            #[name(button)]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "action-row__button",
                set_hexpand: true,

                #[name(shell)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_valign: gtk::Align::Center,
                    add_css_class: "action-row__content-shell",

                    #[name(leading)]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 0,
                        set_valign: gtk::Align::Center,
                        add_css_class: "action-row__leading",
                    },

                    #[name(content)]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,
                        set_hexpand: true,
                        set_valign: gtk::Align::Center,
                        add_css_class: "action-row__content",

                        #[name(title)]
                        gtk::Label {
                            #[watch]
                            set_label: &model.title,
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_hexpand: true,
                            add_css_class: "action-row__title",
                        },

                        #[name(subtitle)]
                        gtk::Label {
                            #[watch]
                            set_label: &model.subtitle,
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            add_css_class: "action-row__subtitle",
                            #[watch]
                            set_visible: !model.subtitle.is_empty(),
                        },
                    },

                    #[name(meta)]
                    gtk::Label {
                        set_valign: gtk::Align::Center,
                        add_css_class: "action-row__meta",
                    },
                }
            },

            #[name(trailing)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 0,
                set_valign: gtk::Align::Center,
                add_css_class: "action-row__trailing",
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ActionRow {
            title: init.title,
            subtitle: init.subtitle,
            variant: init.variant,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let ActionRowInput::Update { title, subtitle } = message;
        self.title = title;
        self.subtitle = subtitle;
    }
}

fn root_classes(variant: RowVariant) -> Vec<&'static str> {
    match variant {
        RowVariant::Default => vec!["action-row"],
        RowVariant::Footer => vec!["action-row", "action-row--footer", "footer-action"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::{Component, ComponentController};

    #[test]
    fn action_row_footer_variant_exposes_row_classes() {
        if gtk::init().is_err() {
            return;
        }

        let component = ActionRow::builder().launch(ActionRowInit {
            title: "Settings".into(),
            subtitle: String::new(),
            variant: RowVariant::Footer,
        });
        let root = component.widget();
        let button = root
            .first_child()
            .and_downcast::<gtk::Button>()
            .expect("action row should have button");
        let shell = button
            .child()
            .and_downcast::<gtk::Box>()
            .expect("action button should contain shell");
        let leading = shell
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("shell should have leading");
        let content = leading
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("shell should have content");
        let title = content
            .first_child()
            .and_downcast::<gtk::Label>()
            .expect("content should have title");
        let subtitle = title
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("content should have subtitle");
        let meta = content
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("shell should have meta");
        let trailing = button
            .next_sibling()
            .and_downcast::<gtk::Box>()
            .expect("action row should have trailing");

        assert!(root.has_css_class("action-row"));
        assert!(root.has_css_class("footer-action"));
        assert!(root.has_css_class("action-row--footer"));
        assert!(button.has_css_class("action-row__button"));
        assert!(shell.has_css_class("action-row__content-shell"));
        assert!(leading.has_css_class("action-row__leading"));
        assert!(content.has_css_class("action-row__content"));
        assert!(title.has_css_class("action-row__title"));
        assert!(subtitle.has_css_class("action-row__subtitle"));
        assert!(meta.has_css_class("action-row__meta"));
        assert!(trailing.has_css_class("action-row__trailing"));
    }
}
