use glimpse::mpris::protocol::{MprisArtwork, MprisPlaybackStatus, MprisPlayer};
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gdk, gio, glib, prelude::*},
};

pub struct MprisPopover {
    popover: gtk::Popover,
    rows_box: gtk::Box,
    empty_label: gtk::Label,
    max_rows: usize,
    show_artwork: bool,
}

pub struct MprisPopoverInit {
    pub parent: gtk::Box,
    pub max_rows: usize,
    pub show_artwork: bool,
}

#[derive(Debug, Clone)]
pub enum MprisPopoverInput {
    Toggle,
    UpdatePlayers(Vec<MprisPlayer>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprisPopoverOutput {
    Previous { player_id: String },
    PlayPause { player_id: String },
    Next { player_id: String },
    Raise { player_id: String },
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

fn visible_players(players: Vec<MprisPlayer>, max_rows: usize) -> Vec<MprisPlayer> {
    players.into_iter().take(max_rows).collect()
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

fn media_button(
    icon_name: &'static str,
    sensitive: bool,
    on_click: impl Fn() + 'static,
) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon_name);
    button.add_css_class("flat");
    button.set_sensitive(sensitive);
    button.connect_clicked(move |_| on_click());
    button
}

fn set_fallback_art(picture: &gtk::Picture, fallback: &gtk::Image) {
    picture.set_paintable(Option::<&gdk::Texture>::None);
    fallback.set_visible(true);
}

fn set_picture_art(picture: &gtk::Picture, fallback: &gtk::Image, texture: &gdk::Texture) {
    picture.set_paintable(Some(texture));
    fallback.set_visible(false);
}

fn load_player_art(picture: &gtk::Picture, fallback: &gtk::Image, artwork: &MprisArtwork) {
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
            glib::spawn_future_local(async move {
                let Ok(response) = reqwest::get(&url).await else {
                    set_fallback_art(&picture, &fallback);
                    return;
                };
                let Ok(bytes) = response.bytes().await else {
                    set_fallback_art(&picture, &fallback);
                    return;
                };
                match gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes.to_vec())) {
                    Ok(texture) => set_picture_art(&picture, &fallback, &texture),
                    Err(_) => set_fallback_art(&picture, &fallback),
                }
            });
        }
        ArtSource::Fallback => set_fallback_art(picture, fallback),
    }
}

fn build_row(
    player: &MprisPlayer,
    sender: &ComponentSender<MprisPopover>,
    show_artwork: bool,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("mpris-card");

    if show_artwork {
        let artwork_box = gtk::Overlay::new();
        artwork_box.add_css_class("mpris-card-art");
        artwork_box.set_valign(gtk::Align::Fill);

        let artwork = gtk::Picture::new();
        artwork.set_halign(gtk::Align::Fill);
        artwork.set_valign(gtk::Align::Fill);
        artwork.set_can_shrink(true);
        artwork.set_keep_aspect_ratio(false);
        artwork_box.set_child(Some(&artwork));

        let fallback = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        fallback.set_halign(gtk::Align::Center);
        fallback.set_valign(gtk::Align::Center);
        fallback.add_css_class("mpris-card-art-fallback");
        artwork_box.add_overlay(&fallback);

        load_player_art(&artwork, &fallback, &player.artwork);
        card.append(&artwork_box);
    }

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_hexpand(true);
    content.add_css_class("mpris-card-content");

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.add_css_class("mpris-card-copy");

    let title = gtk::Label::new(Some(&player_title(player)));
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    title.add_css_class("mpris-card-title");
    text.append(&title);

    let artist = gtk::Label::new(Some(&player.subtitle));
    artist.set_halign(gtk::Align::Start);
    artist.set_xalign(0.0);
    artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
    artist.set_wrap(true);
    artist.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    artist.add_css_class("mpris-card-subtitle");
    artist.set_visible(!player.subtitle.is_empty());
    text.append(&artist);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    controls.set_valign(gtk::Align::Center);
    controls.add_css_class("mpris-card-controls");

    let output = sender.clone();
    let player_id = player.player_id.clone();
    let previous = media_button("media-skip-backward-symbolic", player.can_go_previous, {
        let output = output.clone();
        let player_id = player_id.clone();
        move || {
            let _ = output.output(MprisPopoverOutput::Previous {
                player_id: player_id.clone(),
            });
        }
    });
    controls.append(&previous);

    let play_pause = media_button(play_pause_icon(player.playback_status), player.can_play_pause, {
        let output = output.clone();
        let player_id = player_id.clone();
        move || {
            let _ = output.output(MprisPopoverOutput::PlayPause {
                player_id: player_id.clone(),
            });
        }
    });
    controls.append(&play_pause);

    let next = media_button("media-skip-forward-symbolic", player.can_go_next, {
        let output = output.clone();
        let player_id = player_id.clone();
        move || {
            let _ = output.output(MprisPopoverOutput::Next {
                player_id: player_id.clone(),
            });
        }
    });
    controls.append(&next);

    let open = gtk::Button::with_label("Open");
    open.add_css_class("flat");
    open.set_sensitive(player.can_raise);
    open.connect_clicked({
        let output = output.clone();
        let player_id = player.player_id.clone();
        move |_| {
            let _ = output.output(MprisPopoverOutput::Raise {
                player_id: player_id.clone(),
            });
        }
    });
    controls.append(&open);

    top.append(&text);
    top.append(&controls);
    content.append(&top);

    if shows_progress(player) {
        let progress = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        progress.add_css_class("mpris-card-progress");

        let start = gtk::Label::new(Some(&format_duration(player.position.unwrap_or(0))));
        start.set_xalign(0.0);
        progress.append(&start);

        let bar = gtk::ProgressBar::new();
        bar.set_hexpand(true);
        bar.set_valign(gtk::Align::Center);
        bar.set_fraction(progress_fraction(
            player.position.unwrap_or(0),
            player.length.unwrap_or(0),
        ));
        progress.append(&bar);

        let end = gtk::Label::new(Some(&format_duration(player.length.unwrap_or(0))));
        end.set_xalign(1.0);
        progress.append(&end);

        content.append(&progress);
    }

    card.append(&content);
    card
}

impl SimpleComponent for MprisPopover {
    type Init = MprisPopoverInit;
    type Input = MprisPopoverInput;
    type Output = MprisPopoverOutput;
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Popover::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("mpris-popover");

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let empty_label = gtk::Label::new(Some("No media players"));
        empty_label.set_halign(gtk::Align::Center);
        empty_label.set_valign(gtk::Align::Center);
        body.append(&empty_label);

        let rows_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        body.append(&rows_box);

        root.set_child(Some(&body));

        let model = MprisPopover {
            popover: root.clone(),
            rows_box,
            empty_label,
            max_rows: init.max_rows,
            show_artwork: init.show_artwork,
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            MprisPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            MprisPopoverInput::UpdatePlayers(players) => {
                let players = visible_players(players, self.max_rows);
                self.empty_label.set_visible(players.is_empty());

                while let Some(child) = self.rows_box.first_child() {
                    self.rows_box.remove(&child);
                }

                for player in &players {
                    self.rows_box
                        .append(&build_row(player, &sender, self.show_artwork));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(position: Option<u64>, length: Option<u64>) -> MprisPlayer {
        MprisPlayer {
            player_id: "spotify".into(),
            bus_name: "org.mpris.MediaPlayer2.spotify".into(),
            identity: "Spotify".into(),
            playback_status: MprisPlaybackStatus::Playing,
            title: "Says".into(),
            artist: "Nils Frahm".into(),
            album: "Spaces".into(),
            panel_label: "Nils Frahm - Says".into(),
            subtitle: "Nils Frahm".into(),
            artwork: MprisArtwork::None,
            position,
            length,
            progress_visible: true,
            can_play_pause: true,
            can_go_previous: true,
            can_go_next: true,
            can_raise: true,
            last_active: 1,
        }
    }

    #[test]
    fn progress_row_is_hidden_when_length_is_missing() {
        assert!(!shows_progress(&player(Some(1), None)));
    }

    #[test]
    fn progress_row_is_visible_when_position_and_length_exist() {
        assert!(shows_progress(&player(Some(1), Some(2))));
    }

    #[test]
    fn remote_artwork_stays_typed() {
        assert_eq!(
            artwork_source(&MprisArtwork::RemoteUrl("https://example.com/cover.jpg".into())),
            ArtSource::Remote("https://example.com/cover.jpg".into())
        );
    }

    #[test]
    fn file_uri_artwork_stays_typed() {
        assert_eq!(
            artwork_source(&MprisArtwork::FileUri("file:///tmp/cover.jpg".into())),
            ArtSource::FileUri("file:///tmp/cover.jpg".into())
        );
    }
}
