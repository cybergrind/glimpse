use glimpse::battery::provider::{BatteryState, BatteryStatus};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct BatteryHero {
    icon_name: String,
    percentage_text: String,
    progress: f64,
    state_text: String,
}

#[derive(Debug)]
pub enum BatteryHeroInput {
    Update(BatteryStatus),
}

#[relm4::component(pub)]
impl SimpleComponent for BatteryHero {
    type Init = ();
    type Input = BatteryHeroInput;
    type Output = ();

    view! {
        gtk::Box {
            add_css_class: "battery-hero",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            gtk::Box {
                add_css_class: "battery-hero-header",
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Image {
                    set_pixel_size: 32,
                    #[watch]
                    set_icon_name: Some(&model.icon_name),
                },

                gtk::Label {
                    add_css_class: "battery-pct",
                    #[watch]
                    set_label: &model.percentage_text,
                },
            },

            gtk::ProgressBar {
                add_css_class: "battery-progress",
                #[watch]
                set_fraction: model.progress,
            },

            gtk::Label {
                add_css_class: "battery-state-text",
                set_halign: gtk::Align::Start,
                #[watch]
                set_label: &model.state_text,
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BatteryHero {
            icon_name: "battery-missing-symbolic".into(),
            percentage_text: "\u{2014}".into(),
            progress: 0.0,
            state_text: String::new(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BatteryHeroInput::Update(status) => {
                self.icon_name = status.icon_name.clone();
                self.percentage_text = format!("{}%", status.percentage);
                self.progress = status.percentage as f64 / 100.0;

                self.state_text = match status.state {
                    BatteryState::Discharging if status.time_to_empty > 0 => {
                        format!(
                            "Discharging \u{2014} {} remaining",
                            format_duration(status.time_to_empty)
                        )
                    }
                    BatteryState::Discharging => "Discharging".into(),
                    BatteryState::Charging if status.time_to_full > 0 => {
                        format!(
                            "Charging \u{2014} {} until full",
                            format_duration(status.time_to_full)
                        )
                    }
                    BatteryState::Charging => "Charging".into(),
                    BatteryState::FullyCharged => "Fully charged".into(),
                    BatteryState::PendingCharge => "Plugged in, not charging".into(),
                    BatteryState::PendingDischarge => "Plugged in".into(),
                    BatteryState::Unknown | BatteryState::Empty => String::new(),
                };
            }
        }
    }
}

fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else {
        format!("{minutes}m")
    }
}
