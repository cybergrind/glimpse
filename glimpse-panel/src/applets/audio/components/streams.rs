use std::collections::HashMap;

use glimpse::providers::audio::AudioStream;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use super::stream_item::{StreamItem, StreamItemInit, StreamItemInput};

pub struct StreamList {
    header_label: gtk::Label,
    content: gtk::Box,
    items: HashMap<u64, Controller<StreamItem>>,
    max_vol: f64,
}

pub struct StreamListInit {
    pub max_vol: f64,
    pub show_streams: bool,
}

#[derive(Debug)]
pub enum StreamListInput {
    Update(Vec<AudioStream>),
}

impl SimpleComponent for StreamList {
    type Init = StreamListInit;
    type Input = StreamListInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 0)
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_visible(init.show_streams);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("device-header");

        let header_label = gtk::Label::new(Some("Apps"));
        header_label.set_hexpand(true);
        header_label.set_halign(gtk::Align::Start);
        header.append(&header_label);

        let chevron = gtk::Label::new(Some("›"));
        chevron.add_css_class("chevron");
        header.append(&chevron);

        let btn = gtk::Button::new();
        btn.set_child(Some(&header));
        btn.add_css_class("flat");
        btn.add_css_class("device-btn");
        root.append(&btn);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content.set_visible(false);
        content.add_css_class("streams-box");
        root.append(&content);

        let content_ref = content.clone();
        let chevron_ref = chevron.clone();
        btn.connect_clicked(move |_| {
            let show = !content_ref.is_visible();
            content_ref.set_visible(show);
            chevron_ref.set_label(if show { "⌄" } else { "›" });
        });

        let model = StreamList {
            header_label,
            content,
            items: HashMap::new(),
            max_vol: init.max_vol,
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let StreamListInput::Update(streams) = msg;

        self.header_label
            .set_label(&format!("Apps ({})", streams.len()));

        let incoming_ids: std::collections::HashSet<u64> =
            streams.iter().map(|s| s.index).collect();

        self.items.retain(|id, ctrl| {
            if incoming_ids.contains(id) {
                true
            } else {
                self.content.remove(ctrl.widget());
                false
            }
        });

        for (i, stream) in streams.iter().enumerate() {
            if let Some(ctrl) = self.items.get(&stream.index) {
                ctrl.emit(StreamItemInput::Update(stream.clone()));
            } else {
                if i > 0 {
                    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
                    sep.add_css_class("stream-divider");
                    self.content.append(&sep);
                }
                let ctrl = StreamItem::builder()
                    .launch(StreamItemInit {
                        stream: stream.clone(),
                        max_vol: self.max_vol,
                    })
                    .detach();
                self.content.append(ctrl.widget());
                self.items.insert(stream.index, ctrl);
            }
        }
    }
}
