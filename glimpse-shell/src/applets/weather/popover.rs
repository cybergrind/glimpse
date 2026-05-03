#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::{animated_popover::AnimatedPopover, popover_shell::PopoverShell},
    services::weather::model::State,
};

use super::components::{
    details::{Details, DetailsInput},
    forecast::{Forecast, ForecastInput, has_forecast_items},
    hero::{Hero, HeroInput},
    hourly::{Hourly, HourlyInput},
};

pub struct Popover {
    animation: AnimatedPopover,
    hourly_separator: gtk::Separator,
    details_separator: gtk::Separator,
    forecast_separator: gtk::Separator,
    hero: Controller<Hero>,
    hourly: Controller<Hourly>,
    details: Controller<Details>,
    forecast: Controller<Forecast>,
}

pub struct PopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PopoverInput {
    Toggle,
    Update(State),
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = PopoverInit;
    type Input = PopoverInput;
    type Output = ();

    view! {
        root = gtk::Popover {
            add_css_class: "weather-popover",
            add_css_class: "popover-size-medium",
            set_hexpand: false,

            #[template]
            PopoverShell {
                #[template_child]
                footer {
                    set_visible: false,
                },

                #[template_child]
                content {
                    #[local_ref]
                    hero_widget -> gtk::Box {},

                    #[local_ref]
                    hourly_separator -> gtk::Separator {},

                    #[local_ref]
                    hourly_widget -> gtk::Box {},

                    #[local_ref]
                    details_separator -> gtk::Separator {},

                    #[local_ref]
                    details_widget -> gtk::Box {},

                    #[local_ref]
                    forecast_separator -> gtk::Separator {},

                    #[local_ref]
                    forecast_widget -> gtk::Box {},
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let hero = Hero::builder().launch(()).detach();
        let hourly = Hourly::builder().launch(()).detach();
        let details = Details::builder().launch(()).detach();
        let forecast = Forecast::builder().launch(()).detach();

        let hero_widget = hero.widget().clone();
        let hourly_widget = hourly.widget().clone();
        let details_widget = details.widget().clone();
        let forecast_widget = forecast.widget().clone();
        let hourly_separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        let details_separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        let forecast_separator = gtk::Separator::new(gtk::Orientation::Horizontal);
        hourly_separator.set_visible(false);
        details_separator.set_visible(false);
        forecast_separator.set_visible(false);

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);

        let model = Popover {
            animation: AnimatedPopover::new(&widgets.root),
            hourly_separator,
            details_separator,
            forecast_separator,
            hero,
            hourly,
            details,
            forecast,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            PopoverInput::Toggle => self.animation.toggle(),
            PopoverInput::Update(state) => {
                let snapshot = ready_snapshot(&state);
                let show_hourly = snapshot.is_some_and(|snapshot| !snapshot.hourly.is_empty());
                let show_details = snapshot.is_some();
                let show_forecast =
                    snapshot.is_some_and(|snapshot| has_forecast_items(&snapshot.forecast));
                self.hourly_separator
                    .set_visible(show_hourly || show_details || show_forecast);
                self.details_separator
                    .set_visible(show_hourly && (show_details || show_forecast));
                self.forecast_separator
                    .set_visible(show_forecast && (show_hourly || show_details));
                self.hero.emit(HeroInput::Update(state.clone()));
                self.hourly.emit(HourlyInput::Update(state.clone()));
                self.details.emit(DetailsInput::Update(state.clone()));
                self.forecast.emit(ForecastInput::Update(state));
            }
        }
    }
}

fn ready_snapshot(state: &State) -> Option<&crate::services::weather::model::Snapshot> {
    match state {
        State::Ready(snapshot) => Some(snapshot),
        State::Unknown | State::Loading | State::Unavailable(_) => None,
    }
}
