use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::calendar::Calendar;
use super::date::Date;
use super::events::Events;
use super::notifications::Notifications;
use super::now_playing::NowPlaying;
use super::weather::Weather;
use super::world::WorldClock;

pub struct Popover {
    popover: gtk::Popover,
    #[allow(dead_code)]
    now_playing: Controller<NowPlaying>,
    #[allow(dead_code)]
    notifications: Controller<Notifications>,
    #[allow(dead_code)]
    date: Controller<Date>,
    #[allow(dead_code)]
    calendar: Controller<Calendar>,
    #[allow(dead_code)]
    events: Controller<Events>,
    #[allow(dead_code)]
    world_clock: Controller<WorldClock>,
    #[allow(dead_code)]
    weather: Controller<Weather>,
}

pub struct Init {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum Input {
    Open,
}

#[relm4::component(pub)]
impl SimpleComponent for Popover {
    type Init = Init;
    type Input = Input;
    type Output = ();

    view! {
        gtk::Popover {}
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        container.add_css_class("clock-popover");

        let left_column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        left_column.add_css_class("clock-popover-left");
        container.append(&left_column);

        let now_playing = NowPlaying::builder().launch(()).detach();
        left_column.append(now_playing.widget());

        let notifications = Notifications::builder().launch(()).detach();
        left_column.append(notifications.widget());

        let right_column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        right_column.add_css_class("clock-popover-right");
        container.append(&right_column);

        let date = Date::builder().launch(()).detach();
        right_column.append(date.widget());

        let calendar = Calendar::builder().launch(()).detach();
        right_column.append(calendar.widget());

        let events = Events::builder().launch(()).detach();
        right_column.append(events.widget());

        let world_clock = WorldClock::builder().launch(()).detach();
        right_column.append(world_clock.widget());

        let weather = Weather::builder().launch(()).detach();
        right_column.append(weather.widget());

        root.set_parent(&init.parent);
        root.set_child(Some(&container));

        let model = Popover {
            popover: root.clone(),
            now_playing,
            notifications,
            date,
            calendar,
            events,
            world_clock,
            weather,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            Input::Open => self.popover.popup(),
        }
    }
}
