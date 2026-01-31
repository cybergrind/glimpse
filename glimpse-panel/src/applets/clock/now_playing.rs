use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};
use zbus::Connection;

pub struct NowPlaying {
    title: String,
    artist: String,
    art_url: Option<String>,
    playing: bool,
}

#[derive(Debug)]
pub enum Input {
    Update {
        title: String,
        artist: String,
        art_url: Option<String>,
        playing: bool,
    },
    PlayPause,
    Next,
    Previous,
}

#[relm4::component(pub)]
impl SimpleComponent for NowPlaying {
    type Init = ();
    type Input = Input;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            add_css_class: "card",
            add_css_class: "now-playing",
            set_spacing: 12,

            #[name = "art_image"]
            gtk::Image {
                add_css_class: "now-playing-art",
                set_pixel_size: 64,
                #[watch]
                set_visible: model.art_url.is_some(),
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_valign: gtk::Align::Center,

                gtk::Label {
                    add_css_class: "heading",
                    add_css_class: "now-playing-title",
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    // set_max_width_chars: 25,
                    set_xalign: 0.0,
                    #[watch]
                    set_label: &model.title,
                    #[watch]
                    set_visible: !model.title.is_empty(),
                },

                gtk::Label {
                    add_css_class: "body",
                    add_css_class: "now-playing-artist",
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_max_width_chars: 25,
                    set_xalign: 0.0,
                    #[watch]
                    set_label: &model.artist,
                    #[watch]
                    set_visible: !model.artist.is_empty(),
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    add_css_class: "now-playing-controls",
                    #[watch]
                    set_visible: !model.title.is_empty(),

                    gtk::Button {
                        set_icon_name: "media-skip-backward-symbolic",
                        add_css_class: "flat",
                        connect_clicked => Input::Previous,
                    },

                    gtk::Button {
                        #[watch]
                        set_icon_name: if model.playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" },
                        add_css_class: "flat",
                        connect_clicked => Input::PlayPause,
                    },

                    gtk::Button {
                        set_icon_name: "media-skip-forward-symbolic",
                        add_css_class: "flat",
                        connect_clicked => Input::Next,
                    },
                },
            },

            gtk::Label {
                add_css_class: "now-playing-empty",
                set_label: "Nothing playing",
                #[watch]
                set_visible: model.title.is_empty(),
            },
        }
    }

    fn init(_: (), root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = NowPlaying {
            title: String::new(),
            artist: String::new(),
            art_url: None,
            playing: false,
        };
        let widgets = view_output!();

        spawn_mpris_listener(sender, widgets.art_image.clone());

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Input, _sender: ComponentSender<Self>) {
        match msg {
            Input::Update {
                title,
                artist,
                art_url,
                playing,
            } => {
                self.title = title;
                self.artist = artist;
                self.art_url = art_url;
                self.playing = playing;
            }
            Input::PlayPause => {
                glib::spawn_future_local(async {
                    let _ = mpris_command("PlayPause").await;
                });
            }
            Input::Next => {
                glib::spawn_future_local(async {
                    let _ = mpris_command("Next").await;
                });
            }
            Input::Previous => {
                glib::spawn_future_local(async {
                    let _ = mpris_command("Previous").await;
                });
            }
        }
    }
}

fn spawn_mpris_listener(sender: ComponentSender<NowPlaying>, art_image: gtk::Image) {
    glib::spawn_future_local(async move {
        let Ok(connection) = Connection::session().await else {
            tracing::error!("failed to connect to session bus");
            return;
        };

        let mut last_art_url: Option<String> = None;

        loop {
            match get_current_player(&connection).await {
                Ok(Some((title, artist, art_url, playing))) => {
                    if art_url != last_art_url {
                        if let Some(ref url) = art_url {
                            load_art_image(&art_image, url);
                        }
                        last_art_url = art_url.clone();
                    }
                    sender.input(Input::Update {
                        title,
                        artist,
                        art_url,
                        playing,
                    });
                }
                Ok(None) => {
                    last_art_url = None;
                    sender.input(Input::Update {
                        title: String::new(),
                        artist: String::new(),
                        art_url: None,
                        playing: false,
                    });
                }
                Err(e) => {
                    tracing::debug!("mpris error: {}", e);
                }
            }
            glib::timeout_future_seconds(2).await;
        }
    });
}

fn load_art_image(image: &gtk::Image, url: &str) {
    if url.starts_with("file://") {
        let path = url.strip_prefix("file://").unwrap_or(url);
        if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, 64, 64, true) {
            image.set_from_pixbuf(Some(&pixbuf));
        }
    }
}

async fn get_current_player(
    connection: &Connection,
) -> Result<Option<(String, String, Option<String>, bool)>, zbus::Error> {
    let proxy = zbus::fdo::DBusProxy::new(connection).await?;
    let names = proxy.list_names().await?;

    for name in names {
        let name_str = name.as_str();
        if name_str.starts_with("org.mpris.MediaPlayer2.") {
            let player_proxy = PlayerProxy::builder(connection)
                .destination(name_str)?
                .build()
                .await?;

            let metadata = player_proxy.metadata().await.unwrap_or_default();
            let status = player_proxy.playback_status().await.unwrap_or_default();

            let title = metadata
                .get("xesam:title")
                .and_then(|v| String::try_from(v.clone()).ok())
                .unwrap_or_default();

            let artist = metadata
                .get("xesam:artist")
                .and_then(|v| <Vec<String>>::try_from(v.clone()).ok())
                .and_then(|v| v.into_iter().next())
                .unwrap_or_default();

            let art_url = metadata
                .get("mpris:artUrl")
                .and_then(|v| String::try_from(v.clone()).ok());

            let playing = status == "Playing";

            if !title.is_empty() {
                return Ok(Some((title, artist, art_url, playing)));
            }
        }
    }

    Ok(None)
}

async fn mpris_command(command: &str) -> Result<(), zbus::Error> {
    let connection = Connection::session().await?;
    let proxy = zbus::fdo::DBusProxy::new(&connection).await?;
    let names = proxy.list_names().await?;

    for name in names {
        let name_str = name.as_str();
        if name_str.starts_with("org.mpris.MediaPlayer2.") {
            let player_proxy = PlayerProxy::builder(&connection)
                .destination(name_str)?
                .build()
                .await?;

            match command {
                "PlayPause" => player_proxy.play_pause().await?,
                "Next" => player_proxy.next().await?,
                "Previous" => player_proxy.previous().await?,
                _ => {}
            }
            break;
        }
    }

    Ok(())
}

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn metadata(
        &self,
    ) -> zbus::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>;

    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
}
