use std::time::Duration;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};
use serde::Deserialize;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Deserialize)]
pub struct ClockConfig {
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "%H:%M".to_string()
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            format: default_format(),
        }
    }
}

pub struct ClockApplet {
    time: String,
    format: String,
    tick_handle: JoinHandle<()>,
}

impl Drop for ClockApplet {
    fn drop(&mut self) {
        self.tick_handle.abort();
    }
}

#[derive(Debug)]
pub enum ClockInput {
    Tick,
    LeftClick,
    RightClick,
    Scroll(f64),
    Hover(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for ClockApplet {
    type Init = ClockConfig;
    type Input = ClockInput;
    type Output = ();

    view! {
        gtk::Box {
            add_css_class: "applet-clock",
            set_margin_start: 4,
            set_margin_end: 4,
            gtk::Label {
                #[watch]
                set_label: &model.time,
            }
        }
    }

    fn init(
        config: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let tick_handle = relm4::spawn({
            let sender = sender.clone();
            async move {
                loop {
                    sender.input(ClockInput::Tick);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        });

        // Click handler
        let click = gtk::GestureClick::new();
        click.set_button(0);
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        click.connect_pressed({
            let sender = sender.input_sender().clone();
            move |gesture, _, _, _| {
                let msg = match gesture.current_button() {
                    1 => ClockInput::LeftClick,
                    3 => ClockInput::RightClick,
                    _ => return,
                };
                let _ = sender.send(msg);
            }
        });
        root.add_controller(click);

        // Scroll handler
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.connect_scroll({
            let sender = sender.input_sender().clone();
            move |_, _, dy| {
                let _ = sender.send(ClockInput::Scroll(dy));
                gtk::glib::Propagation::Stop
            }
        });
        root.add_controller(scroll);

        // Hover handler
        let hover = gtk::EventControllerMotion::new();
        hover.connect_enter({
            let sender = sender.input_sender().clone();
            move |_, _, _| {
                let _ = sender.send(ClockInput::Hover(true));
            }
        });
        hover.connect_leave({
            let sender = sender.input_sender().clone();
            move |_| {
                let _ = sender.send(ClockInput::Hover(false));
            }
        });
        root.add_controller(hover);

        let time = {
            use std::fmt::Write;
            let mut buf = String::new();
            let _ = write!(buf, "{}", chrono::Local::now().format(&config.format));
            buf
        };
        let model = ClockApplet {
            time,
            format: config.format,
            tick_handle,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            ClockInput::Tick => {
                use std::fmt::Write;
                let mut buf = String::new();
                match write!(buf, "{}", chrono::Local::now().format(&self.format)) {
                    Ok(_) => self.time = buf,
                    Err(e) => tracing::error!("invalid clock format '{}': {}", self.format, e),
                }
            }
            ClockInput::LeftClick => {
                tracing::debug!("clock left click");
            }
            ClockInput::RightClick => {
                tracing::debug!("clock right click");
            }
            ClockInput::Scroll(dy) => {
                tracing::debug!("clock scroll: {}", dy);
            }
            ClockInput::Hover(entered) => {
                tracing::debug!("clock hover: {}", entered);
            }
        }
    }
}
