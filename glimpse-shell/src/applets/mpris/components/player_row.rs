#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    factory::{DynamicIndex, FactoryComponent, FactorySender},
    gtk::{self, gdk, prelude::*},
};

use crate::{
    applets::mpris::{format, popover::PopoverOutput},
    services::mpris::{Artwork, PlaybackStatus, Player},
};

#[derive(Debug)]
pub struct PlayerRow {
    player: Player,
    show_artwork: bool,
    artwork: Option<gdk::Texture>,
}

#[derive(Debug, Clone)]
pub struct PlayerRowInit {
    pub player: Player,
    pub show_artwork: bool,
}

#[derive(Debug)]
pub enum PlayerRowInput {
    Update { player: Player, show_artwork: bool },
    PlayPausePressed,
    RaisePressed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerRowOutput {
    PlayPause { player_id: String },
    Raise { player_id: String },
}

#[relm4::component(pub)]
impl SimpleComponent for PlayerRow {
    type Init = PlayerRowInit;
    type Input = PlayerRowInput;
    type Output = PlayerRowOutput;

    view! {
        root = gtk::Box {
            add_css_class: "mpris-player-row",
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 10,
            set_hexpand: true,

            gtk::Box {
                add_css_class: "mpris-player-row__art",
                set_size_request: (40, 40),
                set_overflow: gtk::Overflow::Hidden,

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
                    set_pixel_size: 18,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_visible: model.artwork.is_none(),
                },
            },

            #[name = "copy_box"]
            gtk::Box {
                add_css_class: "mpris-player-row__copy",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,

                gtk::Label {
                    add_css_class: "mpris-player-row__title",
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    #[watch]
                    set_label: &format::title(&model.player),
                },

                gtk::Label {
                    add_css_class: "mpris-player-row__subtitle",
                    set_halign: gtk::Align::Start,
                    set_xalign: 0.0,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    #[watch]
                    set_label: &format::subtitle(&model.player),
                },
            },

            gtk::Label {
                add_css_class: "mpris-player-row__status",
                set_valign: gtk::Align::Center,
                #[watch]
                set_label: format::playback_status_text(model.player.playback_status),
            },

            gtk::Button {
                add_css_class: "flat",
                add_css_class: "mpris-player-row__play",
                set_valign: gtk::Align::Center,
                #[watch]
                set_sensitive: model.player.can_play_pause,
                #[watch]
                set_icon_name: play_pause_icon(model.player.playback_status),
                connect_clicked[sender] => move |_| {
                    sender.input(PlayerRowInput::PlayPausePressed);
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let artwork = load_artwork(&init.player, init.show_artwork);
        let model = PlayerRow {
            player: init.player,
            show_artwork: init.show_artwork,
            artwork,
        };
        let widgets = view_output!();
        add_raise_click_controller(&widgets.artwork, sender.clone());
        add_raise_click_controller(&widgets.fallback_icon, sender.clone());
        add_raise_click_controller(&widgets.copy_box, sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            PlayerRowInput::Update {
                player,
                show_artwork,
            } => {
                let artwork_changed =
                    self.show_artwork != show_artwork || self.player.artwork != player.artwork;
                self.show_artwork = show_artwork;
                if artwork_changed {
                    self.artwork = load_artwork(&player, self.show_artwork);
                }
                self.player = player;
            }
            PlayerRowInput::PlayPausePressed => {
                let _ = sender.output(PlayerRowOutput::PlayPause {
                    player_id: self.player.player_id.clone(),
                });
            }
            PlayerRowInput::RaisePressed => {
                if self.player.can_raise {
                    let _ = sender.output(PlayerRowOutput::Raise {
                        player_id: self.player.player_id.clone(),
                    });
                }
            }
        }
    }

    fn post_view() {
        artwork.set_paintable(model.artwork.as_ref());
    }
}

#[derive(Debug, Clone)]
pub struct PlayerRowItemInit {
    pub player: Player,
    pub show_artwork: bool,
}

pub struct PlayerRowItem {
    player_id: String,
    row: Controller<PlayerRow>,
}

impl PlayerRowItem {
    pub fn key(&self) -> &str {
        &self.player_id
    }

    pub fn sync_player(&mut self, player: Player, show_artwork: bool) {
        debug_assert_eq!(self.player_id, player.player_id);
        self.row.emit(PlayerRowInput::Update {
            player,
            show_artwork,
        });
    }
}

impl FactoryComponent for PlayerRowItem {
    type Init = PlayerRowItemInit;
    type Input = PlayerRowInput;
    type Output = PopoverOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = ();
    type Index = DynamicIndex;

    fn init_model(init: Self::Init, _index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let player_id = init.player.player_id.clone();
        let row = PlayerRow::builder()
            .launch(PlayerRowInit {
                player: init.player,
                show_artwork: init.show_artwork,
            })
            .forward(sender.output_sender(), row_output_to_popover_output);

        Self { player_id, row }
    }

    fn init_root(&self) -> Self::Root {
        self.row.widget().clone()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        _root: Self::Root,
        _returned_widget: &gtk::Widget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
    }

    fn update(&mut self, message: Self::Input, _sender: FactorySender<Self>) {
        if let PlayerRowInput::Update {
            player,
            show_artwork,
        } = message
        {
            self.player_id = player.player_id.clone();
            self.sync_player(player, show_artwork);
        }
    }
}

fn row_output_to_popover_output(output: PlayerRowOutput) -> PopoverOutput {
    match output {
        PlayerRowOutput::PlayPause { player_id } => PopoverOutput::PlayPause { player_id },
        PlayerRowOutput::Raise { player_id } => PopoverOutput::Raise { player_id },
    }
}

fn add_raise_click_controller<W: IsA<gtk::Widget>>(widget: &W, sender: ComponentSender<PlayerRow>) {
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.connect_pressed(move |_, _, _, _| {
        sender.input(PlayerRowInput::RaisePressed);
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
}
