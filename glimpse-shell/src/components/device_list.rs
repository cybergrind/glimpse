#![allow(unused_assignments)]

use std::fmt::Debug;

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque},
    gtk::{self, prelude::*},
};

use crate::components::device_status::DeviceStatusView;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceListItem<Command> {
    pub id: String,
    pub icon: String,
    pub label: String,
    pub status: String,
    pub busy: bool,
    pub tooltip: Option<String>,
    pub active: bool,
    pub visible: bool,
    pub command: Command,
}

pub struct DeviceList<Command>
where
    Command: Clone + Debug + Send + 'static,
{
    header: Option<String>,
    items: Vec<DeviceListItem<Command>>,
    rows: FactoryVecDeque<DeviceListRowItem<Command>>,
}

#[derive(Debug)]
pub struct DeviceListInit<Command> {
    pub header: Option<String>,
    pub items: Vec<DeviceListItem<Command>>,
}

#[derive(Debug)]
pub enum DeviceListInput<Command> {
    Update(Vec<DeviceListItem<Command>>),
}

#[derive(Debug)]
enum DeviceRowInput<Command> {
    Update(DeviceListItem<Command>),
    Activate,
}

struct DeviceRow<Command> {
    item: DeviceListItem<Command>,
}

#[relm4::component]
impl<Command> SimpleComponent for DeviceRow<Command>
where
    Command: Clone + Debug + Send + 'static,
{
    type Init = DeviceListItem<Command>;
    type Input = DeviceRowInput<Command>;
    type Output = Command;

    view! {
        root = gtk::Box {
            add_css_class: "device-list-row",
            add_css_class: "action-row",
            #[watch]
            set_visible: model.item.visible,

            #[name(button)]
            gtk::Button {
                add_css_class: "flat",
                add_css_class: "device-list-row__button",
                add_css_class: "action-row__button",
                #[watch]
                set_tooltip_text: model.item.tooltip.as_deref(),
                connect_clicked[sender] => move |_| {
                    sender.input(DeviceRowInput::Activate);
                },

                gtk::Box {
                    add_css_class: "device-list-row__content-shell",
                    add_css_class: "action-row__content-shell",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,

                    gtk::Image {
                        add_css_class: "device-list-row__icon",
                        add_css_class: "action-row__leading",
                        set_pixel_size: 16,
                        #[watch]
                        set_icon_name: Some(&model.item.icon),
                    },

                    gtk::Label {
                        add_css_class: "device-list-row__label",
                        add_css_class: "action-row__title",
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        #[watch]
                        set_label: &model.item.label,
                    },

                    #[name = "status"]
                    #[template]
                    DeviceStatusView {},
                }
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = DeviceRow { item: init };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            DeviceRowInput::Update(item) => {
                self.item = item;
            }
            DeviceRowInput::Activate => {
                let _ = sender.output(self.item.command.clone());
            }
        }
    }

    fn post_view() {
        if model.item.active {
            root.add_css_class("is-active");
            root.add_css_class("is-selected");
        } else {
            root.remove_css_class("is-active");
            root.remove_css_class("is-selected");
        }

        status.set_visible(model.item.busy || !model.item.status.is_empty());
        status.spinner.set_visible(model.item.busy);
        status.spinner.set_spinning(model.item.busy);
        status
            .label
            .set_visible(!model.item.busy && !model.item.status.is_empty());
        status.label.set_label(&model.item.status);
    }
}

struct DeviceListRowItem<Command>
where
    Command: Clone + Debug + Send + 'static,
{
    key: String,
    row: Controller<DeviceRow<Command>>,
}

impl<Command> DeviceListRowItem<Command>
where
    Command: Clone + Debug + Send + 'static,
{
    fn key(&self) -> &str {
        &self.key
    }

    fn sync_item(&mut self, item: DeviceListItem<Command>) {
        self.key = item.id.clone();
        self.row.emit(DeviceRowInput::Update(item));
    }
}

impl<Command> FactoryComponent for DeviceListRowItem<Command>
where
    Command: Clone + Debug + Send + 'static,
{
    type Init = DeviceListItem<Command>;
    type Input = DeviceRowInput<Command>;
    type Output = Command;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let key = init.id.clone();
        let row = DeviceRow::builder()
            .launch(init)
            .forward(sender.output_sender(), |output| output);

        Self { key, row }
    }

    fn init_root(&self) -> Self::Root {
        self.row.widget().clone()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        _root: Self::Root,
        _returned_widget: &gtk::Widget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
    }

    fn update(&mut self, message: Self::Input, _sender: FactorySender<Self>) {
        if let DeviceRowInput::Update(item) = message {
            self.sync_item(item);
        }
    }
}

#[relm4::component(pub)]
impl<Command> SimpleComponent for DeviceList<Command>
where
    Command: Clone + Debug + PartialEq + Eq + Send + 'static,
{
    type Init = DeviceListInit<Command>;
    type Input = DeviceListInput<Command>;
    type Output = Command;

    view! {
        root = gtk::Box {
            add_css_class: "device-list",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            gtk::Box {
                add_css_class: "device-list__header",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                #[watch]
                set_visible: model.header.is_some(),

                gtk::Label {
                    add_css_class: "device-list__title",
                    set_halign: gtk::Align::Start,
                    #[watch]
                    set_label: model.header.as_deref().unwrap_or(""),
                },
            },

            gtk::ScrolledWindow {
                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                set_max_content_height: 300,
                set_propagate_natural_height: true,

                #[local_ref]
                rows_box -> gtk::Box {},
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        rows_box.add_css_class("device-list__body");
        let rows = FactoryVecDeque::builder()
            .launch(rows_box.clone())
            .forward(sender.output_sender(), |output| output);

        let mut model = DeviceList {
            header: init.header,
            items: Vec::new(),
            rows,
        };
        model.sync_rows(init.items);

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            DeviceListInput::Update(items) => {
                if self.items != items {
                    self.sync_rows(items);
                }
            }
        }
    }
}

impl<Command> DeviceList<Command>
where
    Command: Clone + Debug + Send + 'static,
{
    fn sync_rows(&mut self, items: Vec<DeviceListItem<Command>>) {
        let next_keys = items.iter().map(|item| item.id.clone()).collect::<Vec<_>>();
        let current_keys = {
            let guard = self.rows.guard();
            guard
                .iter()
                .map(DeviceListRowItem::key)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        };

        let mut guard = self.rows.guard();
        for op in row_sync_ops(&current_keys, &next_keys) {
            match op {
                RowSyncOp::Move { from, to } => guard.move_to(from, to),
                RowSyncOp::Insert { at } => {
                    guard.insert(at, items[at].clone());
                }
                RowSyncOp::Remove { at } => {
                    guard.remove(at);
                }
            }
        }

        for (index, item) in items.iter().cloned().enumerate() {
            guard[index].sync_item(item);
        }
        drop(guard);

        self.items = items;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowSyncOp {
    Move { from: usize, to: usize },
    Insert { at: usize },
    Remove { at: usize },
}

fn row_sync_ops(current_keys: &[String], next_keys: &[String]) -> Vec<RowSyncOp> {
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
            working.insert(target_index, key.clone());
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
        let ops = row_sync_ops(
            &["a".into(), "b".into(), "c".into()],
            &["b".into(), "d".into(), "a".into()],
        );

        assert_eq!(
            ops,
            vec![
                RowSyncOp::Move { from: 1, to: 0 },
                RowSyncOp::Insert { at: 1 },
                RowSyncOp::Remove { at: 3 },
            ]
        );
    }

    #[test]
    fn item_model_captures_shared_device_row_shape() {
        let item = DeviceListItem {
            id: "device-1".into(),
            icon: "bluetooth-symbolic".into(),
            label: "Headphones".into(),
            status: "75%".into(),
            busy: false,
            tooltip: Some("Connected".into()),
            active: true,
            visible: true,
            command: "connect",
        };

        assert_eq!(item.label, "Headphones");
        assert_eq!(item.status, "75%");
        assert!(item.active);
    }
}
