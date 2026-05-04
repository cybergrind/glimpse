use relm4::{
    ComponentParts, ComponentSender, SimpleComponent, WidgetTemplate,
    gtk::{self, prelude::*},
};
use std::collections::HashMap;

pub struct KeyValueGrid {
    values: Vec<KeyValueItem>,
    rows: Vec<KeyValueRowState>,
    root: gtk::Box,
}

struct KeyValueRowState {
    key: String,
    row: KeyValueRow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValueItem {
    pub label: String,
    pub value: String,
    pub visible: bool,
}

pub fn static_key_value_grid(values: Vec<KeyValueItem>) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("key-value-grid");

    for value in values {
        let row = KeyValueRow::init(value);
        root.append(row.as_ref());
    }

    root
}

#[derive(Debug)]
pub struct KeyValueGridInit {
    pub values: Vec<KeyValueItem>,
}

#[derive(Debug)]
pub enum KeyValueGridInput {
    Update(Vec<KeyValueItem>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UpdatePlan {
    Unchanged,
    ValuesChanged(Vec<usize>),
    ShapeChanged,
}

#[relm4::widget_template]
impl WidgetTemplate for KeyValueRow {
    type Init = KeyValueItem;

    view! {
        gtk::Box {
            add_css_class: "key-value-grid__row",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_visible: init.visible,

            #[name = "key_label"]
            gtk::Label {
                add_css_class: "key-value-grid__key",
                set_label: &init.label,
                set_hexpand: true,
                set_halign: gtk::Align::Start,
            },

            #[name = "value_label"]
            gtk::Label {
                add_css_class: "key-value-grid__value",
                set_label: &init.value,
            },
        },
    }
}

#[relm4::component(pub)]
impl SimpleComponent for KeyValueGrid {
    type Init = KeyValueGridInit;
    type Input = KeyValueGridInput;
    type Output = ();

    view! {
        root = gtk::Box {
            add_css_class: "key-value-grid",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut rows = Vec::with_capacity(init.values.len());
        for value in &init.values {
            let row = build_row(value);
            root.append(row.row.as_ref());
            rows.push(row);
        }

        let model = KeyValueGrid {
            values: init.values,
            rows,
            root: root.clone(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            KeyValueGridInput::Update(values) => self.update_values(values),
        }
    }
}

impl KeyValueGrid {
    fn update_values(&mut self, values: Vec<KeyValueItem>) {
        match plan_update(&self.values, &values) {
            UpdatePlan::Unchanged => {}
            UpdatePlan::ValuesChanged(changed) => {
                for index in changed {
                    if let Some(row) = self.rows.get(index) {
                        row.row.as_ref().set_visible(values[index].visible);
                        row.row.key_label.set_label(&values[index].label);
                        row.row.value_label.set_label(&values[index].value);
                    }
                }
                self.values = values;
            }
            UpdatePlan::ShapeChanged => self.reconcile_rows(values),
        }
    }

    fn reconcile_rows(&mut self, values: Vec<KeyValueItem>) {
        for row in &self.rows {
            self.root.remove(row.row.as_ref());
        }

        let mut existing: HashMap<String, KeyValueRowState> = self
            .rows
            .drain(..)
            .map(|row| (row.key.clone(), row))
            .collect();
        let mut rows = Vec::with_capacity(values.len());

        for value in &values {
            let mut row = existing
                .remove(&value.label)
                .unwrap_or_else(|| build_row(value));
            row.key = value.label.clone();
            row.row.as_ref().set_visible(value.visible);
            row.row.key_label.set_label(&value.label);
            row.row.value_label.set_label(&value.value);
            self.root.append(row.row.as_ref());
            rows.push(row);
        }

        self.values = values;
        self.rows = rows;
    }
}

fn build_row(value: &KeyValueItem) -> KeyValueRowState {
    KeyValueRowState {
        key: value.label.clone(),
        row: KeyValueRow::init(value.clone()),
    }
}

fn plan_update(previous: &[KeyValueItem], next: &[KeyValueItem]) -> UpdatePlan {
    if previous == next {
        return UpdatePlan::Unchanged;
    }

    let same_shape = previous.len() == next.len()
        && previous
            .iter()
            .zip(next)
            .all(|(previous, next)| previous.label == next.label);

    if !same_shape {
        return UpdatePlan::ShapeChanged;
    }

    UpdatePlan::ValuesChanged(
        previous
            .iter()
            .zip(next)
            .enumerate()
            .filter_map(|(index, (previous, next))| (previous != next).then_some(index))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str, value: &str) -> KeyValueItem {
        KeyValueItem {
            label: label.into(),
            value: value.into(),
            visible: true,
        }
    }

    #[test]
    fn unchanged_values_are_noop() {
        let previous = vec![item("Health", "98%"), item("Model", "BAT0")];
        let next = previous.clone();

        assert_eq!(plan_update(&previous, &next), UpdatePlan::Unchanged);
    }

    #[test]
    fn same_shape_updates_only_changed_values() {
        let previous = vec![item("Health", "98%"), item("Model", "BAT0")];
        let next = vec![item("Health", "99%"), item("Model", "BAT0")];

        assert_eq!(
            plan_update(&previous, &next),
            UpdatePlan::ValuesChanged(vec![0])
        );
    }

    #[test]
    fn same_shape_updates_visibility_changes() {
        let previous = vec![item("Health", "98%"), item("Model", "BAT0")];
        let mut next = previous.clone();
        next[1].visible = false;

        assert_eq!(
            plan_update(&previous, &next),
            UpdatePlan::ValuesChanged(vec![1])
        );
    }

    #[test]
    fn added_removed_or_reordered_values_reconcile_rows() {
        let previous = vec![item("Health", "98%"), item("Model", "BAT0")];

        assert_eq!(
            plan_update(
                &previous,
                &[
                    item("Health", "98%"),
                    item("Model", "BAT0"),
                    item("Rate", "5W")
                ]
            ),
            UpdatePlan::ShapeChanged
        );
        assert_eq!(
            plan_update(&previous, &[item("Health", "98%")]),
            UpdatePlan::ShapeChanged
        );
        assert_eq!(
            plan_update(&previous, &[item("Model", "BAT0"), item("Health", "98%")]),
            UpdatePlan::ShapeChanged
        );
    }
}
