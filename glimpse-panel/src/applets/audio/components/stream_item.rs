use glimpse::providers::audio::{AudioProvider, AudioStream};
use relm4::{ComponentParts, ComponentSender, SimpleComponent, gtk::{self, glib, prelude::*}};

pub struct StreamItem {
    name_label: gtk::Label,
    mute_btn: gtk::Button,
    scale: gtk::Scale,
}

pub struct StreamItemInit {
    pub stream: AudioStream,
    pub max_vol: f64,
}

#[derive(Debug)]
pub enum StreamItemInput {
    Update(AudioStream),
    ToggleMute(u64),
    SetVolume(u64, u32),
}

impl SimpleComponent for StreamItem {
    type Init = StreamItemInit;
    type Input = StreamItemInput;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Vertical, 2)
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.add_css_class("stream-row");

        let name_label = gtk::Label::new(Some(&init.stream.app_name));
        name_label.set_halign(gtk::Align::Start);
        name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name_label.set_max_width_chars(30);
        name_label.add_css_class("stream-name");
        root.append(&name_label);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let mute_icon = if init.stream.muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        let mute_btn = gtk::Button::from_icon_name(mute_icon);
        mute_btn.add_css_class("flat");
        mute_btn.add_css_class("mute-btn");
        let idx = init.stream.index;
        mute_btn.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(StreamItemInput::ToggleMute(idx))
        });
        row.append(&mute_btn);

        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, init.max_vol, 1.0);
        scale.set_value(init.stream.volume as f64);
        scale.set_hexpand(true);
        scale.connect_change_value(move |_, _, val| {
            sender.input(StreamItemInput::SetVolume(idx, val as u32));
            glib::Propagation::Proceed
        });
        row.append(&scale);

        root.append(&row);
        root.set_tooltip_text(Some(&format!(
            "{} — {}%",
            init.stream.app_name, init.stream.volume
        )));

        let model = StreamItem { name_label, mute_btn, scale };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            StreamItemInput::Update(stream) => {
                self.name_label.set_label(&stream.app_name);
                self.mute_btn.set_icon_name(if stream.muted {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                });
                if !is_dragging(&self.scale) {
                    self.scale.set_value(stream.volume as f64);
                }
                self.scale
                    .parent()
                    .and_then(|p| p.parent())
                    .map(|root| {
                        root.set_tooltip_text(Some(&format!(
                            "{} — {}%",
                            stream.app_name, stream.volume
                        )))
                    });
            }
            StreamItemInput::ToggleMute(idx) => {
                let target = idx.to_string();
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new().toggle_mute(&target).await {
                        tracing::warn!("audio: stream toggle_mute: {e}");
                    }
                });
            }
            StreamItemInput::SetVolume(idx, vol) => {
                let target = idx.to_string();
                glib::spawn_future_local(async move {
                    if let Err(e) = AudioProvider::new().set_volume(&target, vol).await {
                        tracing::warn!("audio: stream set_volume: {e}");
                    }
                });
            }
        }
    }
}

fn is_dragging(scale: &gtk::Scale) -> bool {
    scale.state_flags().contains(gtk::StateFlags::ACTIVE)
}
