#![allow(unused_assignments)]

use std::collections::HashMap;

use chrono::{Datelike, Days, Local, NaiveDate, Weekday};
use glimpse::calendar::protocol::CalendarMonthSnapshot;
use relm4::{
    Component, ComponentParts, ComponentSender,
    factory::{DynamicIndex, FactorySender, FactoryVecDeque, Position, positions::GridPosition},
    gtk::{self, prelude::*},
};

pub struct Calendar {
    selected_date: NaiveDate,
    visible_month: NaiveDate,
    month_label: String,
    dots_by_day: HashMap<NaiveDate, Vec<String>>,
    day_cells: FactoryVecDeque<CalendarDayItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CalendarCell {
    date: NaiveDate,
    day_label: String,
    dots_markup: String,
    other_month: bool,
    today: bool,
    selected: bool,
}

pub struct CalendarInit {
    pub selected_date: NaiveDate,
}

#[derive(Debug)]
pub enum Input {
    PrevMonth,
    NextMonth,
    SelectDate(NaiveDate),
    SetDate(NaiveDate),
    MonthData(CalendarMonthSnapshot),
    ClearMonth,
}

#[derive(Debug)]
pub enum Output {
    SelectedDate(NaiveDate),
    LoadMonth { year: i32, month: u32 },
}

struct CalendarDayItem {
    cell: CalendarCell,
}

#[derive(Debug)]
enum CalendarDayItemInput {
    Update(CalendarCell),
    Activate,
}

impl CalendarDayItem {
    fn sync_cell(&mut self, cell: CalendarCell) {
        self.cell = cell;
    }
}

impl Position<GridPosition, DynamicIndex> for CalendarDayItem {
    fn position(&self, index: &DynamicIndex) -> GridPosition {
        grid_position_for_index(index.current_index())
    }
}

#[relm4::factory]
impl relm4::factory::FactoryComponent for CalendarDayItem {
    type Init = CalendarCell;
    type Input = CalendarDayItemInput;
    type Output = NaiveDate;
    type CommandOutput = ();
    type ParentWidget = gtk::Grid;
    type Index = DynamicIndex;

    view! {
        root = gtk::Button {
            #[watch]
            set_css_classes: &calendar_day_classes(&self.cell),
            connect_clicked => CalendarDayItemInput::Activate,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,

                #[name(number)]
                gtk::Label {
                    add_css_class: "calendar-day-number",
                    #[watch]
                    set_label: &self.cell.day_label,
                },

                #[name(dots)]
                gtk::Label {
                    add_css_class: "calendar-day-dots",
                    set_use_markup: true,
                    #[watch]
                    set_markup: &self.cell.dots_markup,
                },
            }
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { cell: init }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            CalendarDayItemInput::Update(cell) => self.sync_cell(cell),
            CalendarDayItemInput::Activate => {
                let _ = sender.output(self.cell.date);
            }
        }
    }
}

#[relm4::component(pub)]
impl Component for Calendar {
    type Init = CalendarInit;
    type Input = Input;
    type Output = Output;
    type CommandOutput = Input;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            add_css_class: "calendar",
            add_controller = gtk::EventControllerScroll {
                set_flags: gtk::EventControllerScrollFlags::VERTICAL,
                connect_scroll[sender] => move |_, _dx, dy| {
                    if dy < 0.0 {
                        sender.input(Input::PrevMonth);
                    } else if dy > 0.0 {
                        sender.input(Input::NextMonth);
                    }
                    gtk::glib::Propagation::Stop
                }
            },

            gtk::Box {
                add_css_class: "calendar-header",

                gtk::Label {
                    add_css_class: "calendar-month-label",
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_label: &model.month_label,
                },
            },

            gtk::Box {
                add_css_class: "calendar-weekdays",
                set_homogeneous: true,

                gtk::Label {
                    add_css_class: "calendar-weekday",
                    set_label: "Mo",
                },
                gtk::Label {
                    add_css_class: "calendar-weekday",
                    set_label: "Tu",
                },
                gtk::Label {
                    add_css_class: "calendar-weekday",
                    set_label: "We",
                },
                gtk::Label {
                    add_css_class: "calendar-weekday",
                    set_label: "Th",
                },
                gtk::Label {
                    add_css_class: "calendar-weekday",
                    set_label: "Fr",
                },
                gtk::Label {
                    add_css_class: "calendar-weekday",
                    set_label: "Sa",
                },
                gtk::Label {
                    add_css_class: "calendar-weekday",
                    set_label: "Su",
                },
            },

            #[local_ref]
            day_grid -> gtk::Grid {
                add_css_class: "calendar-grid",
                set_column_homogeneous: true,
                set_row_homogeneous: true,
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let visible_month = month_start(init.selected_date);
        let day_cells = FactoryVecDeque::builder()
            .launch_default()
            .forward(sender.input_sender(), Input::SelectDate);

        let model = Calendar {
            selected_date: init.selected_date,
            visible_month,
            month_label: visible_month.format("%B %Y").to_string(),
            dots_by_day: HashMap::new(),
            day_cells,
        };

        let day_grid = model.day_cells.widget();
        let widgets = view_output!();
        let mut model = model;

        model.sync_day_cells();
        model.refresh_month(&sender);

        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        msg: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(msg, sender, root);
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            Input::PrevMonth => {
                self.visible_month = shift_month(self.visible_month, -1);
                self.selected_date = clamp_day(self.visible_month, self.selected_date.day());
                self.month_label = self.visible_month.format("%B %Y").to_string();
                self.sync_day_cells();
                self.refresh_month(&sender);
                let _ = sender.output(Output::SelectedDate(self.selected_date));
            }
            Input::NextMonth => {
                self.visible_month = shift_month(self.visible_month, 1);
                self.selected_date = clamp_day(self.visible_month, self.selected_date.day());
                self.month_label = self.visible_month.format("%B %Y").to_string();
                self.sync_day_cells();
                self.refresh_month(&sender);
                let _ = sender.output(Output::SelectedDate(self.selected_date));
            }
            Input::SelectDate(date) => {
                self.selected_date = date;
                let next_visible = month_start(date);
                let month_changed = next_visible != self.visible_month;
                self.visible_month = next_visible;
                self.month_label = self.visible_month.format("%B %Y").to_string();
                self.sync_day_cells();
                if month_changed {
                    self.refresh_month(&sender);
                }
                let _ = sender.output(Output::SelectedDate(date));
            }
            Input::SetDate(date) => {
                self.selected_date = date;
                let next_visible = month_start(date);
                let month_changed = next_visible != self.visible_month;
                self.visible_month = next_visible;
                self.month_label = self.visible_month.format("%B %Y").to_string();
                self.sync_day_cells();
                if month_changed {
                    self.refresh_month(&sender);
                }
            }
            Input::MonthData(month) => {
                if month.year != self.visible_month.year()
                    || month.month != self.visible_month.month()
                {
                    return;
                }
                self.dots_by_day = month
                    .days
                    .into_iter()
                    .filter_map(|day| day.date.to_naive_date().map(|date| (date, day.colors)))
                    .collect();
                self.sync_day_cells();
            }
            Input::ClearMonth => {
                self.dots_by_day.clear();
                self.sync_day_cells();
            }
        }
    }
}

impl Calendar {
    fn refresh_month(&self, sender: &ComponentSender<Self>) {
        let _ = sender.output(Output::LoadMonth {
            year: self.visible_month.year(),
            month: self.visible_month.month(),
        });
    }

    fn sync_day_cells(&mut self) {
        let cells = build_calendar_cells(
            self.visible_month,
            self.selected_date,
            Local::now().date_naive(),
            &self.dots_by_day,
        );

        let mut guard = self.day_cells.guard();
        if guard.is_empty() {
            for cell in cells {
                guard.push_back(cell);
            }
            return;
        }

        debug_assert_eq!(guard.len(), cells.len());
        guard.drop();

        for (index, cell) in cells.into_iter().enumerate() {
            self.day_cells
                .send(index, CalendarDayItemInput::Update(cell));
        }
    }
}

fn calendar_day_classes(cell: &CalendarCell) -> Vec<&'static str> {
    let mut classes = vec!["flat", "calendar-day"];
    if cell.other_month {
        classes.push("other-month");
    }
    if cell.today {
        classes.push("today");
    }
    if cell.selected {
        classes.push("selected");
    }
    classes
}

fn grid_position_for_index(index: usize) -> GridPosition {
    GridPosition {
        column: (index % 7) as i32,
        row: (index / 7) as i32,
        width: 1,
        height: 1,
    }
}

fn build_calendar_cells(
    visible_month: NaiveDate,
    selected_date: NaiveDate,
    today: NaiveDate,
    dots_by_day: &HashMap<NaiveDate, Vec<String>>,
) -> Vec<CalendarCell> {
    let grid_start = grid_start(visible_month);

    (0..42u64)
        .map(|offset| {
            let date = grid_start
                .checked_add_days(Days::new(offset))
                .unwrap_or(grid_start);
            CalendarCell {
                date,
                day_label: date.day().to_string(),
                dots_markup: dots_markup(dots_by_day.get(&date)),
                other_month: date.month() != visible_month.month(),
                today: date == today,
                selected: date == selected_date,
            }
        })
        .collect()
}

fn dots_markup(colors: Option<&Vec<String>>) -> String {
    colors
        .map(|colors| {
            colors
                .iter()
                .take(3)
                .map(|color| format!("<span foreground=\"{color}\">•</span>"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

fn shift_month(date: NaiveDate, delta: i32) -> NaiveDate {
    let mut year = date.year();
    let mut month = date.month() as i32 + delta;
    while month < 1 {
        year -= 1;
        month += 12;
    }
    while month > 12 {
        year += 1;
        month -= 12;
    }
    NaiveDate::from_ymd_opt(year, month as u32, 1).unwrap_or(date)
}

fn clamp_day(month: NaiveDate, preferred_day: u32) -> NaiveDate {
    let max_day = days_in_month(month);
    NaiveDate::from_ymd_opt(month.year(), month.month(), preferred_day.min(max_day))
        .unwrap_or(month)
}

fn days_in_month(month: NaiveDate) -> u32 {
    let next = shift_month(month, 1);
    next.checked_sub_days(Days::new(1))
        .map(|day| day.day())
        .unwrap_or(31)
}

fn grid_start(month: NaiveDate) -> NaiveDate {
    let first = month_start(month);
    let offset = match first.weekday() {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    };
    first.checked_sub_days(Days::new(offset)).unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_calendar_cells_includes_selected_today_and_dots() {
        let visible_month = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let selected_date = NaiveDate::from_ymd_opt(2026, 4, 10).unwrap();
        let today = selected_date;
        let mut dots_by_day = HashMap::new();
        dots_by_day.insert(selected_date, vec!["#68a3ff".into(), "#f6c343".into()]);

        let cells = build_calendar_cells(visible_month, selected_date, today, &dots_by_day);

        assert_eq!(cells.len(), 42);
        let selected = cells
            .iter()
            .find(|cell| cell.date == selected_date)
            .expect("selected cell");
        assert!(selected.selected);
        assert!(selected.today);
        assert!(!selected.other_month);
        assert!(selected.dots_markup.contains("#68a3ff"));
        assert!(selected.dots_markup.contains("#f6c343"));
        assert!(cells.iter().any(|cell| cell.other_month));
    }

    #[test]
    fn calendar_day_item_uses_grid_position_from_index() {
        let position = grid_position_for_index(9);

        assert_eq!(position.column, 2);
        assert_eq!(position.row, 1);
        assert_eq!(position.width, 1);
        assert_eq!(position.height, 1);
    }
}
