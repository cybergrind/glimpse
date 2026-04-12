#![allow(unused_assignments)]

use relm4::{
    factory::FactoryVecDeque,
    gtk::{self, prelude::*},
    ComponentParts, ComponentSender, SimpleComponent,
};

use super::indicator_item::{PagerIndicatorItem, PagerIndicatorItemInit, PagerIndicatorItemInput};
use crate::applets::pager::PagerStyle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerIndicatorView {
    pub index: u32,
    pub is_focused: bool,
    pub occupied: bool,
    pub is_urgent: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PagerStripView {
    pub indicators: Vec<PagerIndicatorView>,
    pub tooltip: String,
}

pub struct PagerIndicatorStrip {
    root: gtk::Box,
    tooltip: String,
    style: PagerStyle,
    indicators: FactoryVecDeque<PagerIndicatorItem>,
}

pub struct PagerIndicatorStripInit {
    pub style: PagerStyle,
}

#[derive(Debug)]
pub enum PagerIndicatorStripInput {
    Render(PagerStripView),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PagerIndicatorStripOutput {
    Click(u32),
    Scroll { dy: f64, horizontal: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RowSyncOp {
    Move { from: usize, to: usize },
    Insert { at: usize },
    Remove { at: usize },
}

#[relm4::component(pub)]
impl SimpleComponent for PagerIndicatorStrip {
    type Init = PagerIndicatorStripInit;
    type Input = PagerIndicatorStripInput;
    type Output = PagerIndicatorStripOutput;

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "applet",
            add_css_class: "pager",

            #[name(indicators_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let scroll = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::HORIZONTAL,
        );
        let scroll_sender = sender.clone();
        scroll.connect_scroll(move |_ctrl, dx, dy| {
            if dx != 0.0 {
                let _ = scroll_sender.output(PagerIndicatorStripOutput::Scroll {
                    dy: dx,
                    horizontal: true,
                });
            } else if dy != 0.0 {
                let _ = scroll_sender.output(PagerIndicatorStripOutput::Scroll {
                    dy,
                    horizontal: false,
                });
            }
            gtk::glib::Propagation::Stop
        });
        root.add_controller(scroll);

        let widgets = view_output!();
        let indicators = FactoryVecDeque::builder()
            .launch(widgets.indicators_box.clone())
            .forward(sender.output_sender(), |output| output);

        let model = PagerIndicatorStrip {
            root: widgets.root.clone(),
            tooltip: String::new(),
            style: init.style,
            indicators,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let PagerIndicatorStripInput::Render(view) = msg;
        self.tooltip = view.tooltip;
        self.root.set_tooltip_text(if self.tooltip.is_empty() {
            None
        } else {
            Some(&self.tooltip)
        });
        self.sync_indicators(view.indicators);
    }
}

impl PagerIndicatorStrip {
    fn sync_indicators(&mut self, next_views: Vec<PagerIndicatorView>) {
        let next_keys = next_views
            .iter()
            .map(|indicator| indicator.index)
            .collect::<Vec<_>>();
        {
            let mut guard = self.indicators.guard();
            let current_keys = guard
                .iter()
                .map(PagerIndicatorItem::key)
                .collect::<Vec<_>>();

            for op in row_sync_ops(&current_keys, &next_keys) {
                match op {
                    RowSyncOp::Move { from, to } => guard.move_to(from, to),
                    RowSyncOp::Insert { at } => {
                        guard.insert(
                            at,
                            PagerIndicatorItemInit {
                                style: self.style.clone(),
                                view: next_views[at].clone(),
                            },
                        );
                    }
                    RowSyncOp::Remove { at } => {
                        guard.remove(at);
                    }
                }
            }
        }

        for (index, view) in next_views.into_iter().enumerate() {
            self.indicators
                .send(index, PagerIndicatorItemInput::Update(view));
        }
    }
}

fn row_sync_ops(current_keys: &[u32], next_keys: &[u32]) -> Vec<RowSyncOp> {
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
    use super::{row_sync_ops, RowSyncOp};

    #[test]
    fn row_sync_ops_inserts_and_removes_by_index() {
        assert_eq!(
            row_sync_ops(&[1, 2, 3], &[1, 3, 4]),
            vec![
                RowSyncOp::Move { from: 2, to: 1 },
                RowSyncOp::Insert { at: 2 },
                RowSyncOp::Remove { at: 3 }
            ]
        );
    }

    #[test]
    fn row_sync_ops_moves_existing_indicators_into_new_order() {
        assert_eq!(
            row_sync_ops(&[1, 2, 3], &[3, 1, 2]),
            vec![RowSyncOp::Move { from: 2, to: 0 }]
        );
    }
}
