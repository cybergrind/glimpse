use glimpse::providers::audio::{AudioStream, volume_icon};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, prelude::*},
};

pub struct StreamItem {
    stream: AudioStream,
    max_volume: f64,
    scale: gtk::Scale,
}

pub struct StreamItemInit {
    pub stream: AudioStream,
    pub max_volume: f64,
}

#[derive(Debug)]
pub enum StreamItemInput {
    Update(AudioStream),
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamItemOutput {
    ToggleMute(u64),
    SetVolume { stream_id: u64, volume: u32 },
}

#[relm4::component(pub)]
impl SimpleComponent for StreamItem {
    type Init = StreamItemInit;
    type Input = StreamItemInput;
    type Output = StreamItemOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 2,
            add_css_class: "stream-row",
            #[watch]
            set_tooltip_text: Some(&stream_tooltip(&model.stream)),

            gtk::Label {
                #[watch]
                set_label: &model.stream.app_name,
                set_halign: gtk::Align::Start,
                set_ellipsize: gtk::pango::EllipsizeMode::End,
                set_max_width_chars: 30,
                add_css_class: "stream-name",
            },

            gtk::Box {
                set_spacing: 8,

                gtk::Button {
                    add_css_class: "flat",
                    add_css_class: "mute-btn",
                    #[watch]
                    set_icon_name: stream_icon_name(&model.stream),
                    connect_clicked[sender, stream_id = model.stream.index] => move |_| {
                        let _ = sender.output(StreamItemOutput::ToggleMute(stream_id));
                    },
                },

                #[name(scale)]
                gtk::Scale {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_draw_value: false,
                    set_hexpand: true,
                    set_range: (0.0, model.max_volume),
                    set_increments: (1.0, 5.0),
                    connect_change_value[sender, stream_id = model.stream.index] => move |_, _, value| {
                        let _ = sender.output(StreamItemOutput::SetVolume {
                            stream_id,
                            volume: value as u32,
                        });
                        gtk::glib::Propagation::Proceed
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = StreamItem {
            stream: init.stream,
            max_volume: init.max_volume,
            scale: gtk::Scale::new(gtk::Orientation::Horizontal, None::<&gtk::Adjustment>),
        };
        let widgets = view_output!();
        let mut model = model;
        model.scale = widgets.scale.clone();
        model.scale.set_value(model.stream.volume as f64);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let StreamItemInput::Update(stream) = msg;
        self.stream = stream;
        if !is_dragging(&self.scale) {
            self.scale.set_value(self.stream.volume as f64);
        }
    }
}

fn is_dragging(scale: &gtk::Scale) -> bool {
    scale.state_flags().contains(gtk::StateFlags::ACTIVE)
}

fn stream_icon_name(stream: &AudioStream) -> &'static str {
    volume_icon(stream.volume, stream.muted)
}

fn stream_tooltip(stream: &AudioStream) -> String {
    format!("{} — {}%", stream.app_name, stream.volume)
}
