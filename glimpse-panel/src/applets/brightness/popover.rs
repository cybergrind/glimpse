#![allow(unused_assignments)]

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::components::{
    display_list::{
        BrightnessDisplayList, BrightnessDisplayListInput, BrightnessDisplayListOutput,
    },
    hero::{BrightnessHero, BrightnessHeroInput},
};

pub struct BrightnessPopover {
    popover: gtk::Popover,
    hero: Controller<BrightnessHero>,
    display_list: Controller<BrightnessDisplayList>,
}

pub struct BrightnessPopoverInit {
    pub parent: gtk::Box,
    pub settings_command: String,
}

#[derive(Debug)]
pub enum BrightnessPopoverInput {
    Toggle,
    UpdateDisplays(Vec<BrightnessDisplay>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrightnessPopoverOutput {
    Opened,
    Closed,
    SetDisplayPercent { display_id: String, percent: u8 },
    OpenSettings,
}

#[relm4::component(pub)]
impl SimpleComponent for BrightnessPopover {
    type Init = BrightnessPopoverInit;
    type Input = BrightnessPopoverInput;
    type Output = BrightnessPopoverOutput;

    view! {
        root = gtk::Popover {
            set_autohide: true,
            add_css_class: "brightness-popover",

            connect_show[sender] => move |_| {
                let _ = sender.output(BrightnessPopoverOutput::Opened);
            },

            connect_closed[sender] => move |_| {
                let _ = sender.output(BrightnessPopoverOutput::Closed);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,

                #[local_ref]
                hero_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                #[local_ref]
                display_list_widget -> gtk::Box {},

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_visible: has_settings_button(&init.settings_command),
                },

                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "settings-btn",
                    set_visible: has_settings_button(&init.settings_command),
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(BrightnessPopoverOutput::OpenSettings);
                    },

                    gtk::Label {
                        set_label: "Display Settings",
                    }
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let hero = BrightnessHero::builder().launch(()).detach();
        let hero_widget = hero.widget().clone();

        let display_list =
            BrightnessDisplayList::builder()
                .launch(())
                .forward(sender.output_sender(), |output| match output {
                    BrightnessDisplayListOutput::SetDisplayPercent {
                        display_id,
                        percent,
                    } => BrightnessPopoverOutput::SetDisplayPercent {
                        display_id,
                        percent,
                    },
                });
        let display_list_widget = display_list.widget().clone();

        let widgets = view_output!();
        widgets.root.set_parent(&init.parent);
        widgets.root.set_autohide(true);
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
