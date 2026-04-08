use glimpse::providers::battery::{BatteryState, BatteryStatus};
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::{self, prelude::*}};

pub struct BatteryHero {
    icon: gtk::Image,
    pct: gtk::Label,
    progress: gtk::ProgressBar,
    state_text: gtk::Label,
}

#[derive(Debug)]
pub enum BatteryHeroInput {
    Update(BatteryStatus),
}

impl SimpleComponent for BatteryHero {
    type Init = ();
    type Input = BatteryHeroInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.add_css_class("battery-hero");

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("battery-hero-header");

        let icon = gtk::Image::from_icon_name("battery-missing-symbolic");
        icon.set_pixel_size(32);
        header.append(&icon);

        let pct = gtk::Label::new(Some("—"));
        pct.add_css_class("battery-pct");
        header.append(&pct);

        root.append(&header);

        let progress = gtk::ProgressBar::new();
        progress.add_css_class("battery-progress");
        root.append(&progress);

        let state_text = gtk::Label::new(None);
        state_text.set_halign(gtk::Align::Start);
        state_text.add_css_class("battery-state-text");
        root.append(&state_text);

        let model = BatteryHero { icon, pct, progress, state_text };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let BatteryHeroInput::Update(status) = msg;

        self.icon.set_icon_name(Some(&status.icon_name));
        self.pct.set_label(&format!("{}%", status.percentage));
        self.progress.set_fraction(status.percentage as f64 / 100.0);

        let text = match status.state {
            BatteryState::Discharging if status.time_to_empty > 0 => {
                format!("Discharging — {} remaining", format_duration(status.time_to_empty))
            }
            BatteryState::Discharging => "Discharging".into(),
            BatteryState::Charging if status.time_to_full > 0 => {
                format!("Charging — {} until full", format_duration(status.time_to_full))
            }
            BatteryState::Charging => "Charging".into(),
            BatteryState::FullyCharged => "Fully charged".into(),
            BatteryState::PendingCharge => "Plugged in, not charging".into(),
            BatteryState::PendingDischarge => "Plugged in".into(),
            BatteryState::Unknown | BatteryState::Empty => String::new(),
        };
        self.state_text.set_label(&text);
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
