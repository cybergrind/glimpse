use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionRowInit {
    pub title: String,
    pub subtitle: String,
    pub meta: String,
    pub icon: Option<String>,
    pub visible: bool,
    pub selectable: bool,
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for ActionRow {
    type Init = ActionRowInit;

    view! {
        gtk::Box {
            add_css_class: "action-row",
            set_visible: init.visible,

            #[name = "button"]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "action-row__button",

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_valign: gtk::Align::Center,
                    add_css_class: "action-row__content-shell",

                    #[name = "icon"]
                    gtk::Image {
                        add_css_class: "action-row__leading",
                        set_icon_name: init.icon.as_deref(),
                        set_pixel_size: 16,
                        set_visible: init.icon.is_some(),
                    },

                    gtk::Box {
                        add_css_class: "action-row__content",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,
                        set_hexpand: true,

                        gtk::Label {
                            add_css_class: "action-row__title",
                            set_label: &init.title,
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                        },

                        gtk::Label {
                            add_css_class: "action-row__subtitle",
                            set_label: &init.subtitle,
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_visible: !init.subtitle.is_empty(),
                        },
                    },

                    gtk::Label {
                        add_css_class: "action-row__meta",
                        set_label: &init.meta,
                        set_visible: !init.meta.is_empty(),
                    },

                    gtk::Image {
                        set_icon_name: Some("object-select-symbolic"),
                        set_pixel_size: 14,
                        set_visible: init.selectable,
                        add_css_class: "action-row__trailing",
                    },
                },
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn action_row_exposes_shared_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let row = ActionRow::init(ActionRowInit {
            title: "Action".into(),
            subtitle: String::new(),
            meta: String::new(),
            icon: None,
            visible: true,
            selectable: false,
        });

        assert!(row.has_css_class("action-row"));
        assert!(row.button.has_css_class("action-row__button"));
        assert!(row.icon.has_css_class("action-row__leading"));
    }
}
