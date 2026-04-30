#![allow(unused_assignments)]

use std::collections::HashMap;

use chrono::{Datelike, Days, Local, NaiveDate, Weekday};
use relm4::{
    Component, ComponentParts, ComponentSender,
    factory::{DynamicIndex, FactorySender, FactoryVecDeque, Position, positions::GridPosition},
    gtk::{self, prelude::*},
};

use crate::{
    applets::clock::format,
    services::calendar_events::{CalendarMonthSnapshot, MonthKey},
};

pub struct Calendar {
    selected_date: NaiveDate,
    visible_month: MonthKey,
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

#[derive(Debug)]
pub enum CalendarInput {
    PreviousMonth,
    NextMonth,
    Today,
    SelectDate(NaiveDate),
    SetDate(NaiveDate),
    SetMonth(Option<CalendarMonthSnapshot>),
}

#[derive(Debug)]
pub enum CalendarOutput {
    SelectedDate(NaiveDate),
    VisibleMonthChanged(MonthKey),
}

struct CalendarDayItem {
    cell: CalendarCell,
}

#[derive(Debug)]
enum CalendarDayItemInput {
    Update(CalendarCell),
    Activate,
}

impl Position<GridPosition, DynamicIndex> for CalendarDayItem {
    fn position(&self, index: &DynamicIndex) -> GridPosition {
        GridPosition {
            column: (index.current_index() % 7) as i32,
            row: (index.current_index() / 7) as i32,
            width: 1,
            height: 1,
        }
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

                gtk::Label {
                    add_css_class: "calendar-day-number",
                    #[watch]
                    set_label: &self.cell.day_label,
                },

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
            CalendarDayItemInput::Update(cell) => self.cell = cell,
            CalendarDayItemInput::Activate => {
                let _ = sender.output(self.cell.date);
            }
        }
    }
}

#[relm4::component(pub)]
impl Component for Calendar {
    type Init = NaiveDate;
    type Input = CalendarInput;
    type Output = CalendarOutput;
    type CommandOutput = ();

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            add_css_class: "calendar",
            add_controller = gtk::EventControllerScroll {
                set_flags: gtk::EventControllerScrollFlags::VERTICAL,
                connect_scroll[sender] => move |_, _dx, dy| {
                    if dy < 0.0 {
                        sender.input(CalendarInput::PreviousMonth);
                    } else if dy > 0.0 {
                        sender.input(CalendarInput::NextMonth);
                    }
                    gtk::glib::Propagation::Stop
                }
            },

            gtk::Box {
                add_css_class: "calendar-header",
                set_spacing: 4,

                gtk::Button {
                    add_css_class: "flat",
                    set_icon_name: "go-previous-symbolic",
                    set_tooltip_text: Some("Previous month"),
                    connect_clicked => CalendarInput::PreviousMonth,
                },

                gtk::Label {
                    add_css_class: "calendar-month-label",
                    set_hexpand: true,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_label: &model.month_label,
                },

                gtk::Button {
                    add_css_class: "flat",
                    set_label: "Today",
                    connect_clicked => CalendarInput::Today,
                },

                gtk::Button {
                    add_css_class: "flat",
                    set_icon_name: "go-next-symbolic",
                    set_tooltip_text: Some("Next month"),
                    connect_clicked => CalendarInput::NextMonth,
                },
            },

            gtk::Box {
                add_css_class: "calendar-weekdays",
                set_homogeneous: true,

                gtk::Label { add_css_class: "calendar-weekday", set_label: "Mo" },
                gtk::Label { add_css_class: "calendar-weekday", set_label: "Tu" },
                gtk::Label { add_css_class: "calendar-weekday", set_label: "We" },
                gtk::Label { add_css_class: "calendar-weekday", set_label: "Th" },
                gtk::Label { add_css_class: "calendar-weekday", set_label: "Fr" },
                gtk::Label { add_css_class: "calendar-weekday", set_label: "Sa" },
                gtk::Label { add_css_class: "calendar-weekday", set_label: "Su" },
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
        selected_date: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let visible_month = MonthKey::from_date(selected_date);
        let day_cells = FactoryVecDeque::builder()
            .launch_default()
            .forward(sender.input_sender(), CalendarInput::SelectDate);
        let model = Calendar {
            selected_date,
            visible_month,
            month_label: format::month_label(visible_month),
            dots_by_day: HashMap::new(),
            day_cells,
        };
        let day_grid = model.day_cells.widget();
        let widgets = view_output!();
        let mut model = model;
        model.sync_day_cells();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            CalendarInput::PreviousMonth => self.move_month(-1, &sender),
            CalendarInput::NextMonth => self.move_month(1, &sender),
            CalendarInput::Today => self.select_date(Local::now().date_naive(), true, &sender),
            CalendarInput::SelectDate(date) => self.select_date(date, true, &sender),
            CalendarInput::SetDate(date) => self.select_date(date, false, &sender),
            CalendarInput::SetMonth(Some(month)) => {
                if month.key != self.visible_month {
                    return;
                }
                self.dots_by_day = month
                    .days
                    .into_iter()
                    .filter_map(|day| day.date.to_naive_date().map(|date| (date, day.colors)))
                    .collect();
                self.sync_day_cells();
            }
            CalendarInput::SetMonth(None) => {
                self.dots_by_day.clear();
                self.sync_day_cells();
            }
        }
    }
}

impl Calendar {
    fn move_month(&mut self, delta: i32, sender: &ComponentSender<Self>) {
        let Some(current) = self.visible_month.to_naive_date() else {
            return;
        };
        let next_month = MonthKey::from_date(shift_month(current, delta));
        let day = self.selected_date.day();
        let Some(next_date) = next_month
            .to_naive_date()
            .map(|month| clamp_day(month, day))
        else {
            return;
        };
        self.visible_month = next_month;
        self.selected_date = next_date;
        self.dots_by_day.clear();
        self.month_label = format::month_label(next_month);
        self.sync_day_cells();
        let _ = sender.output(CalendarOutput::VisibleMonthChanged(next_month));
        let _ = sender.output(CalendarOutput::SelectedDate(next_date));
    }

    fn select_date(&mut self, date: NaiveDate, emit: bool, sender: &ComponentSender<Self>) {
        let next_month = MonthKey::from_date(date);
        let month_changed = next_month != self.visible_month;
        self.selected_date = date;
        self.visible_month = next_month;
        self.month_label = format::month_label(next_month);
        if month_changed {
            self.dots_by_day.clear();
            let _ = sender.output(CalendarOutput::VisibleMonthChanged(next_month));
        }
        self.sync_day_cells();
        if emit {
            let _ = sender.output(CalendarOutput::SelectedDate(date));
        }
    }

    fn sync_day_cells(&mut self) {
        let Some(visible_month) = self.visible_month.to_naive_date() else {
            return;
        };
        let cells = build_calendar_cells(
            visible_month,
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
        guard.drop();

        for (index, cell) in cells.into_iter().enumerate() {
            self.day_cells
                .send(index, CalendarDayItemInput::Update(cell));
        }
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

fn dots_markup(colors: Option<&Vec<String>>) -> String {
    colors
        .map(|colors| {
            colors
                .iter()
                .take(3)
                .map(|color| {
                    if is_safe_hex_color(color) {
                        format!("<span foreground=\"{color}\">•</span>")
                    } else {
                        "•".to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn is_safe_hex_color(color: &str) -> bool {
    color.len() == 7
        && color.starts_with('#')
        && color[1..].chars().all(|ch| ch.is_ascii_hexdigit())
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
    let offset = match month.weekday() {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    };
    month.checked_sub_days(Days::new(offset)).unwrap_or(month)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_markup_renders_fallback_dot_for_events_without_color() {
        assert_eq!(dots_markup(Some(&vec![String::new()])), "•");
    }

    #[test]
    fn dots_markup_accepts_safe_hex_colors_only() {
        let markup = dots_markup(Some(&vec!["#aabbcc".into(), "\"bad\"".into()]));

        assert!(markup.contains("foreground=\"#aabbcc\""));
        assert!(!markup.contains("\"bad\""));
    }
}
