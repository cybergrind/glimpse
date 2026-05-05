#![allow(unused_assignments)]

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque},
    gtk::{self, glib, prelude::*},
};

use glimpse_core::services::brightness::{BrightnessSource, BrightnessSourceKind, Command};

const ROW_COMMAND_INTERVAL: Duration = Duration::from_millis(80);
const BRIGHTNESS_ECHO_GRACE: Duration = Duration::from_secs(2);

pub struct SourceSection {
    sources: Vec<BrightnessSource>,
    rows: FactoryVecDeque<SourceRowItem>,
}

pub struct SourceSectionInit {
    pub sources: Vec<BrightnessSource>,
}

#[derive(Debug)]
pub enum SourceSectionInput {
    Update(Vec<BrightnessSource>),
}

#[derive(Debug)]
pub enum SourceControlInput {
    Update(BrightnessSource),
    SetPercent(f64),
}

pub struct SourceControl {
    source: BrightnessSource,
    updating: Rc<Cell<bool>>,
    last_sent: Rc<Cell<Instant>>,
    pending: Rc<Cell<bool>>,
    pending_percent: Rc<Cell<u8>>,
    pending_service_percent: Rc<RefCell<Option<PendingBrightness>>>,
}

#[derive(Debug, Clone)]
struct PendingBrightness {
    percent: u8,
    changed_at: Instant,
}

impl PendingBrightness {
    fn new(percent: u8) -> Self {
        Self {
            percent,
            changed_at: Instant::now(),
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for SourceControl {
    type Init = BrightnessSource;
    type Input = SourceControlInput;
    type Output = Command;

    view! {
        root = gtk::Box {
            add_css_class: "brightness-control",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            set_valign: gtk::Align::Center,
            #[watch]
            set_sensitive: model.source.writable,
            #[watch]
            set_tooltip_text: Some(&format!("{} - {}%", model.source.name, model.source.percent)),

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Box {
                    add_css_class: "brightness-control__icon-ghost",
                },

                gtk::Label {
                    add_css_class: "brightness-control__name",
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                    set_hexpand: true,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    #[watch]
                    set_label: &model.source.name,
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_valign: gtk::Align::Center,
                set_spacing: 8,

                gtk::Image {
                    add_css_class: "brightness-control__icon",
                    add_css_class: "brightness-control__icon-slot",
                    set_pixel_size: 16,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_icon_name: Some(&model.source.icon),
                },

                #[name = "scale"]
                gtk::Scale {
                    add_css_class: "brightness-control__scale",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_draw_value: false,
                    set_hexpand: true,
                    set_valign: gtk::Align::Center,
                    set_range: (0.0, 100.0),
                    set_increments: (1.0, 10.0),
                    set_digits: 0,
                    connect_change_value[sender] => move |_, _, value| {
                        sender.input(SourceControlInput::SetPercent(value));
                        glib::Propagation::Stop
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = SourceControl {
            source: init,
            updating: Rc::new(Cell::new(false)),
            last_sent: Rc::new(Cell::new(Instant::now() - ROW_COMMAND_INTERVAL)),
            pending: Rc::new(Cell::new(false)),
            pending_percent: Rc::new(Cell::new(0)),
            pending_service_percent: Rc::new(RefCell::new(None)),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            SourceControlInput::Update(source) => {
                self.apply_service_source(source);
            }
            SourceControlInput::SetPercent(value) => {
                if self.updating.get() {
                    return;
                }

                let percent = percent_from_scale_value(&self.source, value);
                if percent == self.source.percent {
                    return;
                }

                self.source.current = current_from_percent(&self.source, percent);
                self.source.percent = percent;
                self.pending_service_percent
                    .borrow_mut()
                    .replace(PendingBrightness::new(percent));
                self.emit_throttled(percent, sender);
            }
        }
    }

    fn post_view() {
        if root.has_css_class("is-primary") != model.source.primary {
            if model.source.primary {
                root.add_css_class("is-primary");
            } else {
                root.remove_css_class("is-primary");
            }
        }

        let upper = scale_upper(&model.source);
        let page_increment = if uses_discrete_scale(&model.source) {
            1.0
        } else {
            10.0
        };
        let adjustment = scale.adjustment();
        if (adjustment.upper() - upper).abs() > f64::EPSILON {
            model.updating.set(true);
            scale.set_range(0.0, upper);
            scale.set_increments(1.0, page_increment);
            model.updating.set(false);
        }

        let value = scale_value(&model.source);
        if (scale.value() - value).abs() > f64::EPSILON {
            model.updating.set(true);
            scale.set_value(value);
            model.updating.set(false);
        }
    }
}

impl SourceControl {
    fn apply_service_source(&mut self, mut source: BrightnessSource) {
        let now = Instant::now();
        let should_apply_value = {
            let mut pending = self.pending_service_percent.borrow_mut();
            should_apply_service_percent(&mut pending, source.percent, now)
        };

        if !should_apply_value {
            source.percent = self.source.percent;
            source.current = self.source.current;
        }

        self.source = source;
    }

    fn emit_throttled(&self, percent: u8, sender: ComponentSender<Self>) {
        self.pending_percent.set(percent);

        let now = Instant::now();
        if now.duration_since(self.last_sent.get()) >= ROW_COMMAND_INTERVAL {
            self.pending.set(false);
            self.last_sent.set(now);
            let _ = sender.output(Command::SetPercent {
                id: self.source.id.clone(),
                percent,
            });
            return;
        }

        if self.pending.get() {
            return;
        }

        self.pending.set(true);
        let delay = ROW_COMMAND_INTERVAL.saturating_sub(now.duration_since(self.last_sent.get()));
        let pending = self.pending.clone();
        let pending_percent = self.pending_percent.clone();
        let last_sent = self.last_sent.clone();
        let id = self.source.id.clone();
        glib::timeout_add_local_once(delay, move || {
            if !pending.get() {
                return;
            }
            pending.set(false);
            last_sent.set(Instant::now());
            let _ = sender.output(Command::SetPercent {
                id,
                percent: pending_percent.get(),
            });
        });
    }
}

struct SourceRowItem {
    key: String,
    row: Controller<SourceControl>,
}

impl SourceRowItem {
    fn key(&self) -> &str {
        &self.key
    }

    fn sync_source(&mut self, source: BrightnessSource) {
        self.key = source.id.clone();
        self.row.emit(SourceControlInput::Update(source));
    }
}

impl FactoryComponent for SourceRowItem {
    type Init = BrightnessSource;
    type Input = SourceControlInput;
    type Output = Command;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let key = init.id.clone();
        let row = SourceControl::builder()
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
        if let SourceControlInput::Update(source) = message {
            self.sync_source(source);
        }
    }
}

#[relm4::component(pub)]
impl SimpleComponent for SourceSection {
    type Init = SourceSectionInit;
    type Input = SourceSectionInput;
    type Output = Command;

    view! {
        root = gtk::Box {
            add_css_class: "brightness-source-section",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 6,
            #[watch]
            set_visible: !model.sources.is_empty(),

            #[local_ref]
            rows_box -> gtk::Box {},
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        rows_box.add_css_class("brightness-source-section__body");
        let rows = FactoryVecDeque::builder()
            .launch(rows_box.clone())
            .forward(sender.output_sender(), |output| output);

        let mut model = SourceSection {
            sources: Vec::new(),
            rows,
        };
        model.sync_sources(init.sources);

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            SourceSectionInput::Update(sources) => {
                if self.sources != sources {
                    self.sync_sources(sources);
                }
            }
        }
    }
}

impl SourceSection {
    fn sync_sources(&mut self, sources: Vec<BrightnessSource>) {
        let next_keys = sources
            .iter()
            .map(|source| source.id.clone())
            .collect::<Vec<_>>();
        let current_keys = {
            let guard = self.rows.guard();
            guard
                .iter()
                .map(SourceRowItem::key)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        };

        let mut guard = self.rows.guard();
        for op in row_sync_ops(&current_keys, &next_keys) {
            match op {
                RowSyncOp::Move { from, to } => guard.move_to(from, to),
                RowSyncOp::Insert { at } => {
                    guard.insert(at, sources[at].clone());
                }
                RowSyncOp::Remove { at } => {
                    guard.remove(at);
                }
            }
        }

        for (index, source) in sources.iter().cloned().enumerate() {
            guard[index].sync_source(source);
        }
        drop(guard);

        self.sources = sources;
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

fn uses_discrete_scale(source: &BrightnessSource) -> bool {
    source.kind == BrightnessSourceKind::Keyboard || source.max <= 20
}

fn scale_upper(source: &BrightnessSource) -> f64 {
    if uses_discrete_scale(source) {
        source.max.max(1) as f64
    } else {
        100.0
    }
}

fn scale_value(source: &BrightnessSource) -> f64 {
    if uses_discrete_scale(source) {
        source.current.min(source.max) as f64
    } else {
        source.percent as f64
    }
}

fn percent_from_scale_value(source: &BrightnessSource, value: f64) -> u8 {
    if uses_discrete_scale(source) {
        percent_from_raw_value(
            value.round().clamp(0.0, source.max.max(1) as f64) as u32,
            source.max,
        )
    } else {
        let min = if source.kind == BrightnessSourceKind::BuiltInDisplay {
            1.0
        } else {
            0.0
        };
        value.round().clamp(min, 100.0) as u8
    }
}

fn current_from_percent(source: &BrightnessSource, percent: u8) -> u32 {
    ((source.max as f64 * percent as f64) / 100.0)
        .round()
        .clamp(0.0, source.max as f64) as u32
}

fn should_apply_service_percent(
    pending: &mut Option<PendingBrightness>,
    service_percent: u8,
    now: Instant,
) -> bool {
    let Some(value) = pending else {
        return true;
    };

    if value.percent == service_percent {
        *pending = None;
        return true;
    }

    if now.duration_since(value.changed_at) < BRIGHTNESS_ECHO_GRACE {
        return false;
    }

    *pending = None;
    true
}

fn percent_from_raw_value(current: u32, max: u32) -> u8 {
    if max == 0 {
        return 0;
    }

    ((current as f64 / max as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse_core::services::brightness::BrightnessSourceKind;

    #[test]
    fn row_sync_ops_preserves_stable_rows() {
        let ops = row_sync_ops(&["a".into(), "b".into()], &["b".into(), "c".into()]);

        assert_eq!(
            ops,
            vec![
                RowSyncOp::Move { from: 1, to: 0 },
                RowSyncOp::Insert { at: 1 },
                RowSyncOp::Remove { at: 2 },
            ]
        );
    }

    #[test]
    fn keyboard_source_uses_native_step_count() {
        let source = source("kbd", BrightnessSourceKind::Keyboard, 1, 3, 33);

        assert_eq!(scale_upper(&source), 3.0);
        assert_eq!(scale_value(&source), 1.0);
        assert_eq!(percent_from_scale_value(&source, 2.0), 67);
    }

    #[test]
    fn display_source_uses_percent_scale() {
        let source = source(
            "display",
            BrightnessSourceKind::BuiltInDisplay,
            128,
            255,
            50,
        );

        assert_eq!(scale_upper(&source), 100.0);
        assert_eq!(scale_value(&source), 50.0);
        assert_eq!(percent_from_scale_value(&source, 42.2), 42);
        assert_eq!(percent_from_scale_value(&source, 0.0), 1);
    }

    #[test]
    fn pending_brightness_ignores_recent_stale_service_values() {
        let mut pending = Some(PendingBrightness::new(80));
        let now = Instant::now();

        assert!(!should_apply_service_percent(&mut pending, 40, now));
        assert!(pending.is_some());
    }

    #[test]
    fn pending_brightness_clears_when_service_catches_up() {
        let mut pending = Some(PendingBrightness::new(80));
        let now = Instant::now();

        assert!(should_apply_service_percent(&mut pending, 80, now));
        assert!(pending.is_none());
    }

    fn source(
        id: &str,
        kind: BrightnessSourceKind,
        current: u32,
        max: u32,
        percent: u8,
    ) -> BrightnessSource {
        BrightnessSource {
            id: id.into(),
            name: id.into(),
            kind,
            icon: "display-brightness-symbolic".into(),
            current,
            max,
            percent,
            writable: true,
            primary: false,
            available: true,
        }
    }
}
