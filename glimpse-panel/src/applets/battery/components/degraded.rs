use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct DegradedWarning {
    label: gtk::Label,
}

#[derive(Debug)]
pub enum DegradedWarningInput {
    Update(String),
}

impl SimpleComponent for DegradedWarning {
    type Init = ();
    type Input = DegradedWarningInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 6)
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.add_css_class("profile-degraded-row");
        root.set_visible(false);

        let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
        icon.set_pixel_size(14);
        root.append(&icon);

        let label = gtk::Label::new(None);
        label.set_halign(gtk::Align::Start);
        label.set_wrap(true);
        label.add_css_class("profile-degraded");
        root.append(&label);

        ComponentParts {
            model: DegradedWarning { label },
            widgets: (),
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let DegradedWarningInput::Update(reason) = msg;
        if reason.is_empty() {
            self.label.parent().map(|p| p.set_visible(false));
        } else {
            self.label
                .set_label(&format!("Performance degraded: {reason}"));
            self.label.parent().map(|p| p.set_visible(true));
        }
    }
}
