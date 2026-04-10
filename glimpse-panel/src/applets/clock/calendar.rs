use std::collections::HashMap;

use chrono::{Datelike, Days, Local, NaiveDate, Weekday};
use glimpse::calendar::protocol::CalendarMonthSnapshot;
use relm4::{
    Component, ComponentParts, ComponentSender,
    gtk::{self, prelude::*},
};

pub struct Calendar {
    selected_date: NaiveDate,
    visible_month: NaiveDate,
    month_label: gtk::Label,
    grid: gtk::Grid,
    dots_by_day: HashMap<NaiveDate, Vec<String>>,
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

            #[name = "header_overlay"]
            gtk::Overlay {
                add_css_class: "calendar-header",
            },

            #[name = "weekday_grid"]
            gtk::Grid {
                add_css_class: "calendar-weekdays",
                set_column_homogeneous: true,
            },

            #[name = "grid"]
            gtk::Grid {
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
        let widgets = view_output!();

        let nav_row = gtk::CenterBox::new();

        let prev_button = gtk::Button::new();
        prev_button.add_css_class("flat");
        prev_button.add_css_class("calendar-nav");
        prev_button.set_label("‹");
        prev_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(Input::PrevMonth)
        });
        nav_row.set_start_widget(Some(&prev_button));

        let next_button = gtk::Button::new();
        next_button.add_css_class("flat");
        next_button.add_css_class("calendar-nav");
        next_button.set_label("›");
        next_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(Input::NextMonth)
        });
        nav_row.set_end_widget(Some(&next_button));

        let month_label = gtk::Label::new(None);
        month_label.add_css_class("calendar-month-label");
        month_label.set_halign(gtk::Align::Center);
        month_label.set_valign(gtk::Align::Center);

        widgets.header_overlay.set_child(Some(&nav_row));
        widgets.header_overlay.add_overlay(&month_label);

        for (idx, label) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
            .into_iter()
            .enumerate()
        {
            let day = gtk::Label::new(Some(label));
            day.add_css_class("calendar-weekday");
            widgets.weekday_grid.attach(&day, idx as i32, 0, 1, 1);
        }

        let mut model = Calendar {
            selected_date: init.selected_date,
            visible_month: month_start(init.selected_date),
            month_label,
            grid: widgets.grid.clone(),
            dots_by_day: HashMap::new(),
        };

        model.render_days(&sender);
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
                self.render_days(&sender);
                self.refresh_month(&sender);
                let _ = sender.output(Output::SelectedDate(self.selected_date));
            }
            Input::NextMonth => {
                self.visible_month = shift_month(self.visible_month, 1);
                self.selected_date = clamp_day(self.visible_month, self.selected_date.day());
                self.render_days(&sender);
                self.refresh_month(&sender);
                let _ = sender.output(Output::SelectedDate(self.selected_date));
            }
            Input::SelectDate(date) => {
                self.selected_date = date;
                let next_visible = month_start(date);
                let month_changed = next_visible != self.visible_month;
                self.visible_month = next_visible;
                self.render_days(&sender);
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
                self.render_days(&sender);
                if month_changed {
                    self.refresh_month(&sender);
                }
            }
            Input::MonthData(month) => {
                if month.year != self.visible_month.year() || month.month != self.visible_month.month()
                {
                    return;
                }
                self.dots_by_day = month
                    .days
                    .into_iter()
                    .filter_map(|day| day.date.to_naive_date().map(|date| (date, day.colors)))
                    .collect();
                self.render_days(&sender);
            }
            Input::ClearMonth => {
                self.dots_by_day.clear();
                self.render_days(&sender);
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

    fn render_days(&mut self, sender: &ComponentSender<Self>) {
        self.month_label
            .set_label(&self.visible_month.format("%B %Y").to_string());

        while let Some(child) = self.grid.first_child() {
            self.grid.remove(&child);
        }

        let grid_start = grid_start(self.visible_month);
        let today = Local::now().date_naive();

        for offset in 0..42u64 {
            let date = grid_start
                .checked_add_days(Days::new(offset))
                .unwrap_or(grid_start);

            let button = gtk::Button::new();
            button.add_css_class("flat");
            button.add_css_class("calendar-day");
            if date.month() != self.visible_month.month() {
                button.add_css_class("other-month");
            }
            if date == today {
                button.add_css_class("today");
            }
            if date == self.selected_date {
                button.add_css_class("selected");
            }

            let content = gtk::Box::new(gtk::Orientation::Vertical, 2);

            let number = gtk::Label::new(Some(&date.day().to_string()));
            number.add_css_class("calendar-day-number");
            content.append(&number);

            let dots = gtk::Label::new(None);
            dots.add_css_class("calendar-day-dots");
            dots.set_use_markup(true);
            dots.set_markup(&dots_markup(self.dots_by_day.get(&date)));
            content.append(&dots);

            button.set_child(Some(&content));
            let sender = sender.clone();
            button.connect_clicked(move |_| {
                sender.input(Input::SelectDate(date));
            });

            self.grid
                .attach(&button, (offset % 7) as i32, (offset / 7) as i32, 1, 1);
        }
    }
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
