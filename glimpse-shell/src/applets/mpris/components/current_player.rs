#![allow(unused_assignments)]

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gdk, prelude::*},
};

use crate::{
    applets::mpris::format,
    services::mpris::{Artwork, PlaybackStatus, Player},
};

#[derive(Debug)]
pub struct CurrentPlayer {
    player: Option<Player>,
    show_artwork: bool,
    artwork: Option<gdk::Texture>,
}

#[derive(Debug)]
pub enum CurrentPlayerInput {
    Update {
        player: Option<Player>,
        show_artwork: bool,
    },
    PreviousPressed,
    PlayPausePressed,
    NextPressed,
    RaisePressed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentPlayerOutput {
    Previous { player_id: String },
    PlayPause { player_id: String },
    Next { player_id: String },
    Raise { player_id: String },
}

#[relm4::component(pub)]
impl SimpleComponent for CurrentPlayer {
    type Init = ();
    type Input = CurrentPlayerInput;
    type Output = CurrentPlayerOutput;

    view! {
        root = gtk::Box {
            add_css_class: "mpris-current-player",
            add_css_class: "card-surface",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 14,
            #[watch]
            set_visible: model.player.is_some(),

            gtk::Box {
                add_css_class: "mpris-current-player__art",
                set_overflow: gtk::Overflow::Hidden,
                set_size_request: (96, 96),

                #[name = "artwork"]
                gtk::Picture {
                    set_can_shrink: true,
                    set_content_fit: gtk::ContentFit::Cover,
                    #[watch]
                    set_visible: model.artwork.is_some(),
                },

                #[name = "fallback_icon"]
                gtk::Image {
                    set_icon_name: Some("audio-x-generic-symbolic"),
                    set_pixel_size: 36,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_visible: model.artwork.is_none(),
                },
            },

            gtk::Box {
                add_css_class: "mpris-current-player__meta",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 10,
                set_hexpand: true,

                #[name = "copy_box"]
                gtk::Box {
                    add_css_class: "mpris-current-player__copy",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 3,
                    set_hexpand: true,

                    gtk::Label {
                        add_css_class: "mpris-current-player__title",
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        #[watch]
                        set_label: &model.player.as_ref().map(format::title).unwrap_or_default(),
                    },

                    gtk::Label {
                        add_css_class: "mpris-current-player__subtitle",
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        #[watch]
                        set_label: &model.player.as_ref().map(format::subtitle).unwrap_or_default(),
                    },
                },

                gtk::Box {
                    add_css_class: "mpris-current-player__progress",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    #[watch]
                    set_visible: model.player.as_ref().is_some_and(shows_progress),

                    gtk::Label {
                        #[watch]
                        set_label: &model.player.as_ref()
                            .and_then(|player| player.position)
                            .map(format::duration)
                            .unwrap_or_default(),
                    },

                    gtk::ProgressBar {
                        set_hexpand: true,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_fraction: model.player.as_ref()
                            .map(progress_fraction)
                            .unwrap_or_default(),
                    },

                    gtk::Label {
                        #[watch]
                        set_label: &model.player.as_ref()
                            .and_then(|player| player.length)
                            .map(format::duration)
                            .unwrap_or_default(),
                    },
                },

                gtk::Box {
                    add_css_class: "mpris-current-player__controls",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_valign: gtk::Align::Center,

                    gtk::Button {
                        add_css_class: "flat",
                        set_icon_name: "media-skip-backward-symbolic",
                        #[watch]
                        set_sensitive: model.player.as_ref().is_some_and(|player| player.can_go_previous),
                        connect_clicked[sender] => move |_| {
                            sender.input(CurrentPlayerInput::PreviousPressed);
                        },
                    },

                    gtk::Button {
                        add_css_class: "flat",
                        add_css_class: "mpris-current-player__play",
                        #[watch]
                        set_sensitive: model.player.as_ref().is_some_and(|player| player.can_play_pause),
                        #[watch]
                        set_icon_name: model.player.as_ref()
                            .map(|player| play_pause_icon(player.playback_status))
                            .unwrap_or("media-playback-start-symbolic"),
                        connect_clicked[sender] => move |_| {
                            sender.input(CurrentPlayerInput::PlayPausePressed);
                        },
                    },

                    gtk::Button {
                        add_css_class: "flat",
                        set_icon_name: "media-skip-forward-symbolic",
                        #[watch]
                        set_sensitive: model.player.as_ref().is_some_and(|player| player.can_go_next),
                        connect_clicked[sender] => move |_| {
                            sender.input(CurrentPlayerInput::NextPressed);
                        },
                    },
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = CurrentPlayer {
            player: None,
            show_artwork: true,
            artwork: None,
        };
        let widgets = view_output!();
        add_raise_click_controller(&widgets.artwork, sender.clone());
        add_raise_click_controller(&widgets.fallback_icon, sender.clone());
        add_raise_click_controller(&widgets.copy_box, sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            CurrentPlayerInput::Update {
                player,
                show_artwork,
            } => {
                let artwork_changed = self.show_artwork != show_artwork
                    || self.player.as_ref().map(|player| &player.artwork)
                        != player.as_ref().map(|player| &player.artwork);
                self.show_artwork = show_artwork;
                if artwork_changed {
                    self.artwork = player
                        .as_ref()
                        .and_then(|player| load_artwork(player, self.show_artwork));
                }
                self.player = player;
            }
            CurrentPlayerInput::PreviousPressed => {
                if let Some(player_id) = self.player_id() {
                    let _ = sender.output(CurrentPlayerOutput::Previous { player_id });
                }
            }
            CurrentPlayerInput::PlayPausePressed => {
                if let Some(player_id) = self.player_id() {
                    let _ = sender.output(CurrentPlayerOutput::PlayPause { player_id });
                }
            }
            CurrentPlayerInput::NextPressed => {
                if let Some(player_id) = self.player_id() {
                    let _ = sender.output(CurrentPlayerOutput::Next { player_id });
                }
            }
            CurrentPlayerInput::RaisePressed => {
                if self.player.as_ref().is_some_and(|player| player.can_raise)
                    && let Some(player_id) = self.player_id()
                {
                    let _ = sender.output(CurrentPlayerOutput::Raise { player_id });
                }
            }
        }
    }

    fn post_view() {
        artwork.set_paintable(model.artwork.as_ref());
    }
}

impl CurrentPlayer {
    fn player_id(&self) -> Option<String> {
        self.player.as_ref().map(|player| player.player_id.clone())
    }
}

fn add_raise_click_controller<W: IsA<gtk::Widget>>(
    widget: &W,
    sender: ComponentSender<CurrentPlayer>,
) {
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.connect_pressed(move |_, _, _, _| {
        sender.input(CurrentPlayerInput::RaisePressed);
    });
    widget.add_controller(click);
}

fn load_artwork(player: &Player, show_artwork: bool) -> Option<gdk::Texture> {
    if !show_artwork {
        return None;
    }

    match &player.artwork {
        Artwork::FilePath(path) => gdk::Texture::from_filename(path).ok(),
        Artwork::FileUri(uri) => gio::File::for_uri(uri)
            .path()
            .and_then(|path| gdk::Texture::from_filename(path).ok()),
        Artwork::RemoteUrl(_) | Artwork::None => None,
    }
}

fn shows_progress(player: &Player) -> bool {
    matches!((player.position, player.length), (Some(_), Some(length)) if player.progress_visible && length > 0)
}

fn progress_fraction(player: &Player) -> f64 {
    format::progress_fraction(player.position.unwrap_or(0), player.length.unwrap_or(0))
}

fn play_pause_icon(status: PlaybackStatus) -> &'static str {
    match status {
        PlaybackStatus::Playing => "media-playback-pause-symbolic",
        PlaybackStatus::Paused | PlaybackStatus::Stopped => "media-playback-start-symbolic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_pause_icon_tracks_status() {
        assert_eq!(
            play_pause_icon(PlaybackStatus::Playing),
            "media-playback-pause-symbolic"
        );
        assert_eq!(
            play_pause_icon(PlaybackStatus::Paused),
            "media-playback-start-symbolic"
        );
    }

    #[test]
    fn progress_fraction_tracks_player_position() {
        let mut player = Player {
            position: Some(25_000_000),
            length: Some(100_000_000),
            progress_visible: true,
            ..Default::default()
        };

        assert_eq!(progress_fraction(&player), 0.25);

        player.position = Some(50_000_000);

        assert_eq!(progress_fraction(&player), 0.5);
    }
}
