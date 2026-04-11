use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use glimpse::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gdk, gio, glib, prelude::*},
};

pub struct MprisPlayerRow {
    artwork_box: gtk::Overlay,
    title: gtk::Label,
    artist: gtk::Label,
    previous: gtk::Button,
    play_pause: gtk::Button,
    next: gtk::Button,
    open: gtk::Button,
    progress_box: gtk::Box,
    progress_start: gtk::Label,
    progress_bar: gtk::ProgressBar,
    progress_end: gtk::Label,
    artwork_picture: gtk::Picture,
    artwork_fallback: gtk::Image,
    artwork_revision: Rc<Cell<u64>>,
    current: Rc<RefCell<MprisPlayer>>,
    player: MprisPlayer,
    current_artwork: MprisArtwork,
    show_artwork: bool,
}

pub struct MprisPlayerRowInit {
    pub player: MprisPlayer,
    pub show_artwork: bool,
}

#[derive(Debug)]
pub enum MprisPlayerRowInput {
    Update(MprisPlayer),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisPlayerRowOutput {
    Previous { player_id: String },
    PlayPause { player_id: String },
    Next { player_id: String },
    Raise { player_id: String },
}

#[relm4::component(pub)]
impl SimpleComponent for MprisPlayerRow {
    type Init = MprisPlayerRowInit;
    type Input = MprisPlayerRowInput;
    type Output = MprisPlayerRowOutput;

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 12,
            add_css_class: "mpris-card",

            #[name(artwork_box)]
            gtk::Overlay {
                add_css_class: "mpris-card-art",

                #[name(artwork_picture)]
                gtk::Picture {
                    set_can_shrink: true,
                    set_keep_aspect_ratio: true,
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Fill,
                },

                #[name(artwork_fallback)]
                gtk::Image {
                    set_icon_name: Some("audio-x-generic-symbolic"),
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    add_css_class: "mpris-card-art-fallback",
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_hexpand: true,
                add_css_class: "mpris-card-content",

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 2,
                        set_hexpand: true,
                        add_css_class: "mpris-card-copy",

                        #[name(title)]
                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_wrap: true,
                            set_wrap_mode: gtk::pango::WrapMode::WordChar,
                            add_css_class: "mpris-card-title",
                        },

                        #[name(artist)]
                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_wrap: true,
                            set_wrap_mode: gtk::pango::WrapMode::WordChar,
                            add_css_class: "mpris-card-subtitle",
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 6,
                        set_valign: gtk::Align::Center,
                        add_css_class: "mpris-card-controls",

                        #[name(previous)]
                        gtk::Button {
                            add_css_class: "flat",
                            set_icon_name: "media-skip-backward-symbolic",
                            connect_clicked[sender, current = current.clone()] => move |_| {
                                let _ = sender.output(MprisPlayerRowOutput::Previous {
                                    player_id: current.borrow().player_id.clone(),
                                });
                            },
                        },

                        #[name(play_pause)]
                        gtk::Button {
                            add_css_class: "flat",
                            connect_clicked[sender, current = current.clone()] => move |_| {
                                let _ = sender.output(MprisPlayerRowOutput::PlayPause {
                                    player_id: current.borrow().player_id.clone(),
                                });
                            },
                        },

                        #[name(next)]
                        gtk::Button {
                            add_css_class: "flat",
                            set_icon_name: "media-skip-forward-symbolic",
                            connect_clicked[sender, current = current.clone()] => move |_| {
                                let _ = sender.output(MprisPlayerRowOutput::Next {
                                    player_id: current.borrow().player_id.clone(),
                                });
                            },
                        },

                        #[name(open)]
                        gtk::Button {
                            add_css_class: "flat",
                            set_label: "Open",
                            connect_clicked[sender, current = current.clone()] => move |_| {
                                let _ = sender.output(MprisPlayerRowOutput::Raise {
                                    player_id: current.borrow().player_id.clone(),
                                });
                            },
                        },
                    },
                },

                #[name(progress_box)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    add_css_class: "mpris-card-progress",

                    #[name(progress_start)]
                    gtk::Label {
                        set_xalign: 0.0,
                    },

                    #[name(progress_bar)]
                    gtk::ProgressBar {
                        set_hexpand: true,
                        set_valign: gtk::Align::Center,
                    },

                    #[name(progress_end)]
                    gtk::Label {
                        set_xalign: 1.0,
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let current = Rc::new(RefCell::new(init.player.clone()));
        let widgets = view_output!();

        let mut model = MprisPlayerRow {
            artwork_box: widgets.artwork_box.clone(),
            title: widgets.title.clone(),
            artist: widgets.artist.clone(),
            previous: widgets.previous.clone(),
            play_pause: widgets.play_pause.clone(),
            next: widgets.next.clone(),
            open: widgets.open.clone(),
            progress_box: widgets.progress_box.clone(),
            progress_start: widgets.progress_start.clone(),
            progress_bar: widgets.progress_bar.clone(),
            progress_end: widgets.progress_end.clone(),
            artwork_picture: widgets.artwork_picture.clone(),
            artwork_fallback: widgets.artwork_fallback.clone(),
            artwork_revision: Rc::new(Cell::new(0)),
            current: current.clone(),
            player: init.player,
            current_artwork: MprisArtwork::None,
            show_artwork: init.show_artwork,
        };
        model.refresh();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        let MprisPlayerRowInput::Update(player) = msg;
        *self.current.borrow_mut() = player.clone();
        self.player = player;
        self.refresh();
    }
}

impl MprisPlayerRow {
    fn refresh(&mut self) {
        self.artwork_box.set_visible(self.show_artwork);
        self.title.set_label(&player_title(&self.player));
        self.artist.set_label(&self.player.subtitle);
        self.artist.set_visible(!self.player.subtitle.is_empty());

        self.previous.set_sensitive(self.player.can_go_previous);
        self.play_pause.set_sensitive(self.player.can_play_pause);
        self.play_pause
            .set_icon_name(play_pause_icon(self.player.playback_status));
        self.next.set_sensitive(self.player.can_go_next);
        self.open.set_sensitive(self.player.can_raise);

        self.refresh_progress();
        self.refresh_artwork();
    }

    fn refresh_progress(&self) {
        let progress_visible = shows_progress(&self.player);
        self.progress_box.set_visible(progress_visible);
        if !progress_visible {
            return;
        }

        let position = self.player.position.unwrap_or(0);
        let length = self.player.length.unwrap_or(0);
        self.progress_start.set_label(&format_duration(position));
        self.progress_bar
            .set_fraction(progress_fraction(position, length));
        self.progress_end.set_label(&format_duration(length));
    }

    fn refresh_artwork(&mut self) {
        if !self.show_artwork {
            return;
        }

        if !artwork_needs_reload(&self.current_artwork, &self.player.artwork) {
            return;
        }

        load_player_art(
            &self.artwork_picture,
            &self.artwork_fallback,
            &self.player.artwork,
            &self.artwork_revision,
        );
        self.current_artwork = self.player.artwork.clone();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ArtSource {
    FilePath(String),
    FileUri(String),
    Remote(String),
    Fallback,
}

fn artwork_source(artwork: &MprisArtwork) -> ArtSource {
    match artwork {
        MprisArtwork::FilePath(path) => ArtSource::FilePath(path.clone()),
        MprisArtwork::FileUri(uri) => ArtSource::FileUri(uri.clone()),
        MprisArtwork::RemoteUrl(url) => ArtSource::Remote(url.clone()),
        MprisArtwork::None => ArtSource::Fallback,
    }
}

fn artwork_needs_reload(current: &MprisArtwork, next: &MprisArtwork) -> bool {
    current != next
}

fn player_title(player: &MprisPlayer) -> String {
    if !player.title.is_empty() {
        player.title.clone()
    } else {
        player.identity.clone()
    }
}

fn play_pause_icon(status: MprisPlaybackStatus) -> &'static str {
    match status {
        MprisPlaybackStatus::Playing => "media-playback-pause-symbolic",
        MprisPlaybackStatus::Paused | MprisPlaybackStatus::Stopped => {
            "media-playback-start-symbolic"
        }
    }
}

fn shows_progress(player: &MprisPlayer) -> bool {
    matches!((player.position, player.length), (Some(_), Some(length)) if player.progress_visible && length > 0)
}

fn format_duration(value_micros: u64) -> String {
    let total_seconds = value_micros / 1_000_000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn progress_fraction(position: u64, length: u64) -> f64 {
    if length == 0 {
        0.0
    } else {
        (position as f64 / length as f64).clamp(0.0, 1.0)
    }
}

fn set_fallback_art(picture: &gtk::Picture, fallback: &gtk::Image) {
    picture.set_paintable(Option::<&gdk::Texture>::None);
    fallback.set_visible(true);
}

fn set_picture_art(picture: &gtk::Picture, fallback: &gtk::Image, texture: &gdk::Texture) {
    picture.set_paintable(Some(texture));
    fallback.set_visible(false);
}

fn load_player_art(
    picture: &gtk::Picture,
    fallback: &gtk::Image,
    artwork: &MprisArtwork,
    revision: &Rc<Cell<u64>>,
) {
    let next_revision = revision.get().wrapping_add(1);
    revision.set(next_revision);

    match artwork_source(artwork) {
        ArtSource::FilePath(path) => {
            let file = gio::File::for_path(path);
            match gdk::Texture::from_file(&file) {
                Ok(texture) => set_picture_art(picture, fallback, &texture),
                Err(_) => set_fallback_art(picture, fallback),
            }
        }
        ArtSource::FileUri(uri) => {
            let file = gio::File::for_uri(&uri);
            match gdk::Texture::from_file(&file) {
                Ok(texture) => set_picture_art(picture, fallback, &texture),
                Err(_) => set_fallback_art(picture, fallback),
            }
        }
        ArtSource::Remote(url) => {
            let picture = picture.clone();
            let fallback = fallback.clone();
            let revision = revision.clone();
            glib::spawn_future_local(async move {
                let requested_revision = next_revision;

                let Ok(response) = reqwest::get(&url).await else {
                    if revision.get() == requested_revision {
                        set_fallback_art(&picture, &fallback);
                    }
                    return;
                };
                let Ok(bytes) = response.bytes().await else {
                    if revision.get() == requested_revision {
                        set_fallback_art(&picture, &fallback);
                    }
                    return;
                };
                if revision.get() != requested_revision {
                    return;
                }
                match gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes.to_vec())) {
                    Ok(texture) => set_picture_art(&picture, &fallback, &texture),
                    Err(_) => set_fallback_art(&picture, &fallback),
                }
            });
        }
        ArtSource::Fallback => set_fallback_art(picture, fallback),
    }
}

#[cfg(test)]
mod tests {
    use glimpse::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};

    use super::{MprisPlayerRowOutput, player_title, shows_progress};

    fn test_player() -> MprisPlayer {
        MprisPlayer {
            player_id: "spotify".into(),
            bus_name: "org.mpris.MediaPlayer2.spotify".into(),
            identity: "Spotify".into(),
            playback_status: MprisPlaybackStatus::Playing,
            title: String::new(),
            artist: "Nils Frahm".into(),
            album: "Spaces".into(),
            panel_label: "Nils Frahm - Says".into(),
            subtitle: "Nils Frahm".into(),
            artwork: MprisArtwork::None,
            position: None,
            length: None,
            progress_visible: false,
            can_play_pause: true,
            can_go_previous: true,
            can_go_next: true,
            can_raise: true,
            last_active: 1,
        }
    }

    #[test]
    fn row_output_variants_are_comparable() {
        assert_eq!(
            MprisPlayerRowOutput::Raise {
                player_id: "spotify".into(),
            },
            MprisPlayerRowOutput::Raise {
                player_id: "spotify".into(),
            }
        );
    }

    #[test]
    fn title_prefers_track_then_identity() {
        let mut player = test_player();
        player.title = "Says".into();
        assert_eq!(player_title(&player), "Says");

        player.title.clear();
        player.identity = "Spotify".into();
        assert_eq!(player_title(&player), "Spotify");
    }

    #[test]
    fn shows_progress_only_with_position_length_and_flag() {
        let mut player = test_player();
        player.progress_visible = true;
        player.position = Some(5);
        player.length = Some(10);
        assert!(shows_progress(&player));

        player.length = None;
        assert!(!shows_progress(&player));
    }
}
