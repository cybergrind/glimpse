use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRowInit {
    pub icon: &'static str,
    pub preview: String,
}

#[relm4::widget_template(pub)]
impl WidgetTemplate for HistoryRow {
    type Init = HistoryRowInit;

    view! {
        gtk::Box {
            add_css_class: "clipboard-row",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            set_valign: gtk::Align::Center,

            #[name = "button"]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "clipboard-row__button",
                set_hexpand: true,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_valign: gtk::Align::Center,

                    #[name = "icon"]
                    gtk::Image {
                        add_css_class: "clipboard-row__icon",
                        set_icon_name: Some(init.icon),
                        set_pixel_size: 16,
                        set_valign: gtk::Align::Center,
                    },

                    #[name = "preview"]
                    gtk::Label {
                        add_css_class: "clipboard-row__preview",
                        set_label: &init.preview,
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_hexpand: true,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 44,
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
    fn history_row_exposes_stable_class_contract() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let row = HistoryRow::init(HistoryRowInit {
            icon: "edit-paste-symbolic",
            preview: "hello".into(),
        });

        assert!(row.has_css_class("clipboard-row"));
        assert!(row.button.has_css_class("clipboard-row__button"));
        assert!(row.icon.has_css_class("clipboard-row__icon"));
        assert!(row.preview.has_css_class("clipboard-row__preview"));
    }
}
