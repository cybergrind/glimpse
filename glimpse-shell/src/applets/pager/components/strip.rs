#![allow(unused_assignments)]

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    factory::FactoryVecDeque,
    gtk::{self, prelude::*},
};

use super::item::{Init as ItemInit, Input as ItemInput, Item};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerItem {
    pub id: usize,
    pub target: PagerTarget,
    pub label: String,
    pub active: bool,
    pub focused: bool,
    pub occupied: bool,
    pub urgent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagerTarget {
    Workspace(usize),
    Window(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    pub visible: bool,
    pub tooltip: String,
    pub items: Vec<PagerItem>,
    pub placeholder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Classic,
    Workspaces,
}

pub struct Strip {
    visible: bool,
    tooltip: String,
    placeholder: bool,
    items: Vec<PagerItem>,
    rows: FactoryVecDeque<Item>,
}

#[derive(Debug)]
pub enum Input {
    Render(View),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Activate(PagerTarget),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowSyncOp {
    Move { from: usize, to: usize },
    Insert { at: usize },
    Remove { at: usize },
}

#[relm4::component(pub)]
impl SimpleComponent for Strip {
    type Init = Kind;
    type Input = Input;
    type Output = Output;

    view! {
        root = gtk::Box {
            add_css_class: "applet",
            add_css_class: "pager",
            add_css_class: kind_css_class(init),
            set_orientation: gtk::Orientation::Horizontal,
            set_valign: gtk::Align::Center,
            #[watch]
            set_visible: model.visible,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            #[local_ref]
            rows_box -> gtk::Box {},

            #[local_ref]
            placeholder_box -> gtk::Box {
                #[watch]
                set_visible: model.placeholder,
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let rows_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        rows_box.set_valign(gtk::Align::Center);
        let placeholder_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        placeholder_box.set_valign(gtk::Align::Center);
        placeholder_box.add_css_class("pager-dot");
        placeholder_box.add_css_class("focused");
        let rows = FactoryVecDeque::builder()
            .launch(rows_box.clone())
            .forward(sender.output_sender(), |output| output);

        let model = Strip {
            visible: true,
            tooltip: String::new(),
            placeholder: false,
            items: Vec::new(),
            rows,
        };

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Render(view) => {
                self.visible = view.visible;
                self.tooltip = view.tooltip;
                self.placeholder = view.placeholder;
                self.sync_rows(view.items);
            }
        }
    }
}

fn kind_css_class(kind: Kind) -> &'static str {
    match kind {
        Kind::Classic => "pager-classic",
        Kind::Workspaces => "pager-workspaces",
    }
}

impl Strip {
    fn sync_rows(&mut self, items: Vec<PagerItem>) {
        if self.items == items {
            return;
        }

        let next_keys = items.iter().map(|item| item.id).collect::<Vec<_>>();
        let current_keys = {
            let guard = self.rows.guard();
            guard.iter().map(Item::key).collect::<Vec<_>>()
        };

        let mut guard = self.rows.guard();
        for op in row_sync_ops(&current_keys, &next_keys) {
            match op {
                RowSyncOp::Move { from, to } => guard.move_to(from, to),
                RowSyncOp::Insert { at } => {
                    guard.insert(
                        at,
                        ItemInit {
                            view: items[at].clone(),
                        },
                    );
                }
                RowSyncOp::Remove { at } => {
                    guard.remove(at);
                }
            }
        }

        drop(guard);

        for (index, item) in items.iter().cloned().enumerate() {
            self.rows.send(index, ItemInput::Update(item));
        }

        self.items = items;
    }
}

fn row_sync_ops(current_keys: &[usize], next_keys: &[usize]) -> Vec<RowSyncOp> {
    let mut working = current_keys.to_vec();
    let mut ops = Vec::new();

    for (target_index, key) in next_keys.iter().enumerate() {
        if working.get(target_index) == Some(key) {
            continue;
        }

        if let Some(found_index) = working.iter().position(|current| current == key) {
            let moved = working.remove(found_index);
            working.insert(target_index, moved);
            ops.push(RowSyncOp::Move {
                from: found_index,
                to: target_index,
            });
        } else {
            working.insert(target_index, *key);
            ops.push(RowSyncOp::Insert { at: target_index });
        }
    }

    while working.len() > next_keys.len() {
        working.remove(next_keys.len());
        ops.push(RowSyncOp::Remove {
            at: next_keys.len(),
        });
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_sync_ops_preserves_move_insert_and_remove_order() {
        assert_eq!(
            row_sync_ops(&[1, 2, 3], &[2, 4, 1]),
            vec![
                RowSyncOp::Move { from: 1, to: 0 },
                RowSyncOp::Insert { at: 1 },
                RowSyncOp::Remove { at: 3 },
            ]
        );
    }
}
