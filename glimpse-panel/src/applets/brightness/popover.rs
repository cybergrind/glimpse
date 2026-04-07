use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use glimpse_client::Client;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use super::applet::{BrightnessDisplay, choose_primary_display, summary_icon_name};

struct BrightnessRow {
    root: gtk::Box,
    percent: gtk::Label,
    scale: gtk::Scale,
    max: Rc<Cell<u32>>,
}

pub struct BrightnessPopover {
    popover: gtk::Popover,
    client: Arc<Client>,
    hero_icon: gtk::Image,
    hero_subtitle: gtk::Label,
    rows_box: gtk::Box,
    rows: HashMap<String, BrightnessRow>,
}

pub struct BrightnessPopoverInit {
    pub parent: gtk::Box,
    pub client: Arc<Client>,
    pub settings_command: String,
}

#[derive(Debug)]
pub enum BrightnessPopoverInput {
    Toggle,
    UpdateDisplays(Vec<BrightnessDisplay>),
}

#[relm4::component(pub)]
impl SimpleComponent for BrightnessPopover {
    type Init = BrightnessPopoverInit;
    type Input = BrightnessPopoverInput;
    type Output = ();

    view! {
        gtk::Popover {}
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("brightness-popover");

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero.add_css_class("brightness-hero");

        let hero_icon = gtk::Image::from_icon_name("display-brightness-high-symbolic");
        hero_icon.set_pixel_size(32);
        hero.append(&hero_icon);

        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let title = gtk::Label::new(Some("Brightness"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("brightness-hero-title");
        title_box.append(&title);

        let hero_subtitle = gtk::Label::new(Some("No controllable displays"));
        hero_subtitle.set_halign(gtk::Align::Start);
        hero_subtitle.add_css_class("brightness-hero-subtitle");
        title_box.append(&hero_subtitle);
        hero.append(&title_box);

        body.append(&hero);
        body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&rows_box);

        if !init.settings_command.is_empty() {
            body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            let cmd = init.settings_command;
            let label = gtk::Label::new(Some("Display Settings"));
            label.set_halign(gtk::Align::Start);
            let btn = gtk::Button::new();
            btn.set_child(Some(&label));
            btn.add_css_class("flat");
            btn.add_css_class("settings-btn");
            btn.connect_clicked(move |_| {
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if let Some((&program, args)) = parts.split_first() {
                    let _ = std::process::Command::new(program).args(args).spawn();
                }
            });
            body.append(&btn);
        }

        root.set_child(Some(&body));

        let model = BrightnessPopover {
            popover: root.clone(),
            client: init.client,
            hero_icon,
            hero_subtitle,
            rows_box,
            rows: HashMap::new(),
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            BrightnessPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            BrightnessPopoverInput::UpdateDisplays(displays) => {
                if let Some(primary) = choose_primary_display(&displays) {
                    self.hero_icon
                        .set_icon_name(Some(summary_icon_name(primary.percentage)));
                    self.hero_subtitle
                        .set_label(&format!("{} • {}%", primary.name, primary.percentage));
                } else {
                    self.hero_icon
                        .set_icon_name(Some("display-brightness-off-symbolic"));
                    self.hero_subtitle.set_label("No controllable displays");
                }

                while let Some(child) = self.rows_box.first_child() {
                    self.rows_box.remove(&child);
                }

                let mut live_ids = Vec::new();
                for display in displays {
                    live_ids.push(display.id.clone());
                    let row = self.rows.entry(display.id.clone()).or_insert_with(|| {
                        build_row(
                            display.id.clone(),
                            display.name.clone(),
                            self.client.clone(),
                        )
                    });
                    update_row(row, &display);
                    self.rows_box.append(&row.root);
                }

                self.rows
                    .retain(|id, _| live_ids.iter().any(|live| live == id));
            }
        }
    }
}

fn build_row(display_id: String, display_name: String, client: Arc<Client>) -> BrightnessRow {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
    root.add_css_class("brightness-row");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let name = gtk::Label::new(Some(&display_name));
    name.set_hexpand(true);
    name.set_halign(gtk::Align::Start);
    name.add_css_class("brightness-row-name");
    header.append(&name);

    let percent = gtk::Label::new(Some("0%"));
    percent.add_css_class("brightness-row-percent");
    header.append(&percent);
    root.append(&header);

    let max = Rc::new(Cell::new(100));
    let scale = build_scale(display_id, client, max.clone());
    root.append(&scale);

    BrightnessRow {
        root,
        percent,
        scale,
        max,
    }
}

fn build_scale(display_id: String, client: Arc<Client>, max: Rc<Cell<u32>>) -> gtk::Scale {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_hexpand(true);

    let last_sent = Rc::new(Cell::new(
        Instant::now() - std::time::Duration::from_millis(200),
    ));
    let pending = Rc::new(Cell::new(false));

    scale.connect_change_value(move |scale, _, value| {
        let display_id = display_id.clone();
        let last_sent = last_sent.clone();
        let pending = pending.clone();
        let now = Instant::now();
        let elapsed = now.duration_since(last_sent.get());
        let send =
            move |display_id: String, value: u32, client: Arc<Client>, max: Rc<Cell<u32>>| {
                let raw_value = (((value as u64) * (max.get().max(1) as u64)) / 100) as u32;
                glib::spawn_future_local(async move {
                    let _ = client
                        .call(
                            "brightness.set",
                            serde_json::json!({"display_id": display_id, "value": raw_value}),
                        )
                        .await;
                });
            };

        if elapsed.as_millis() >= 100 {
            last_sent.set(now);
            pending.set(false);
            send(display_id, value as u32, client.clone(), max.clone());
        } else if !pending.get() {
            pending.set(true);
            let scale = scale.clone();
            let client = client.clone();
            let max = max.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(100), move || {
                if pending.get() {
                    pending.set(false);
                    last_sent.set(Instant::now());
                    send(display_id, scale.value() as u32, client, max);
                }
            });
        }

        glib::Propagation::Proceed
    });

    scale
}

fn update_row(row: &BrightnessRow, display: &BrightnessDisplay) {
    row.max.set(display.max);
    row.percent.set_label(&format!("{}%", display.percentage));
    if !row.scale.state_flags().contains(gtk::StateFlags::ACTIVE) {
        row.scale.set_value(display.percentage as f64);
    }
}
