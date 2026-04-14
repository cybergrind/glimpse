use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

#[derive(Debug, Clone, Default)]
pub struct DetailGridInit {
    pub rows: Vec<(String, String)>,
}

pub struct DetailGrid {
    rows: Vec<(String, String)>,
    container: gtk::Box,
}

#[derive(Debug)]
pub enum DetailGridInput {
    ReplaceRows(Vec<(String, String)>),
}

#[relm4::component(pub)]
impl SimpleComponent for DetailGrid {
    type Init = DetailGridInit;
    type Input = DetailGridInput;
    type Output = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "detail-grid",
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        render_rows(&root, &init.rows);
        let model = DetailGrid {
            rows: init.rows,
            container: root.clone(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        let DetailGridInput::ReplaceRows(rows) = message;
        self.rows = rows;
        render_rows(&self.container, &self.rows);
    }
}

fn render_rows(container: &gtk::Box, rows: &[(String, String)]) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    for (key, value) in rows {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("detail-grid__row");

        let key_label = gtk::Label::new(Some(key));
        key_label.add_css_class("detail-grid__key");
        key_label.set_halign(gtk::Align::Start);
        key_label.set_xalign(0.0);
        key_label.set_hexpand(true);

        let value_label = gtk::Label::new(Some(value));
        value_label.add_css_class("detail-grid__value");
        value_label.set_halign(gtk::Align::End);
        value_label.set_xalign(1.0);

        row.append(&key_label);
        row.append(&value_label);
        container.append(&row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use relm4::{Component, ComponentController};

    #[test]
    fn detail_grid_renders_row_key_and_value_classes() {
        if gtk::init().is_err() {
            return;
        }

        let component = DetailGrid::builder().launch(DetailGridInit {
            rows: vec![("State".into(), "Connected".into())],
        });
        let root = component.widget();
        let row = root
            .first_child()
            .and_downcast::<gtk::Box>()
            .expect("detail grid should have a row");
        let key = row
            .first_child()
            .and_downcast::<gtk::Label>()
            .expect("detail row should have key label");
        let value = key
            .next_sibling()
            .and_downcast::<gtk::Label>()
            .expect("detail row should have value label");

        assert!(root.has_css_class("detail-grid"));
        assert!(row.has_css_class("detail-grid__row"));
        assert!(key.has_css_class("detail-grid__key"));
        assert!(value.has_css_class("detail-grid__value"));
    }
}
