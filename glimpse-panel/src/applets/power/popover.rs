use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct PowerPopover {
    popover: gtk::Popover,
    profiles_box: gtk::Box,
}

pub struct PowerPopoverInit {
    pub parent: gtk::Box,
}

#[derive(Debug)]
pub enum PowerPopoverInput {
    Toggle,
    Update {
        profiles: Vec<String>,
        active: String,
    },
}

#[derive(Debug, Clone)]
pub enum PowerPopoverOutput {
    SetProfile(String),
    Suspend,
    Hibernate,
    Reboot,
    PowerOff,
}

#[relm4::component(pub)]
impl SimpleComponent for PowerPopover {
    type Init = PowerPopoverInit;
    type Input = PowerPopoverInput;
    type Output = PowerPopoverOutput;

    view! {
        gtk::Popover {}
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("power-popover");

        let section_label = gtk::Label::new(Some("Power Profile"));
        section_label.set_halign(gtk::Align::Start);
        section_label.add_css_class("section-title");
        body.append(&section_label);

        let profiles_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&profiles_box);

        body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        for (label, output) in [
            ("Suspend", PowerPopoverOutput::Suspend),
            ("Hibernate", PowerPopoverOutput::Hibernate),
        ] {
            let btn = gtk::Button::with_label(label);
            btn.add_css_class("flat");
            btn.add_css_class("action-row");
            let s = sender.clone();
            btn.connect_clicked(move |_| {
                s.output(output.clone()).ok();
            });
            body.append(&btn);
        }

        body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        for (label, output) in [
            ("Reboot\u{2026}", PowerPopoverOutput::Reboot),
            ("Power Off\u{2026}", PowerPopoverOutput::PowerOff),
        ] {
            let btn = gtk::Button::with_label(label);
            btn.add_css_class("flat");
            btn.add_css_class("action-row");
            let s = sender.clone();
            btn.connect_clicked(move |_| {
                s.output(output.clone()).ok();
            });
            body.append(&btn);
        }

        root.set_parent(&init.parent);
        root.set_child(Some(&body));

        let model = PowerPopover {
            popover: root.clone(),
            profiles_box,
        };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PowerPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            PowerPopoverInput::Update { profiles, active } => {
                while let Some(child) = self.profiles_box.first_child() {
                    self.profiles_box.remove(&child);
                }

                let mut group: Option<gtk::CheckButton> = None;
                for name in &profiles {
                    let display = name
                        .split('-')
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");

                    let btn = gtk::CheckButton::with_label(&display);
                    if let Some(g) = &group {
                        btn.set_group(Some(g));
                    }
                    btn.set_active(name == &active);
                    btn.add_css_class("profile-row");

                    let profile = name.clone();
                    let s = sender.clone();
                    btn.connect_toggled(move |b: &gtk::CheckButton| {
                        if b.is_active() {
                            s.output(PowerPopoverOutput::SetProfile(profile.clone()))
                                .ok();
                        }
                    });

                    self.profiles_box.append(&btn);
                    if group.is_none() {
                        group = Some(btn);
                    }
                }
            }
        }
    }
}
