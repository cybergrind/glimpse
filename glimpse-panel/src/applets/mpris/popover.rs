use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
};

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
    rows: HashMap<String, PlayerRowWidgets>,
}

struct PlayerRowWidgets {
    root: gtk::Box,
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
    artwork_picture: Option<gtk::Picture>,
    artwork_fallback: Option<gtk::Image>,
    artwork_revision: Option<Rc<Cell<u64>>>,
    current_artwork: MprisArtwork,
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

fn configure_artwork_picture(picture: &gtk::Picture) {
    picture.set_halign(gtk::Align::Fill);
    picture.set_valign(gtk::Align::Fill);
    picture.set_can_shrink(true);
    picture.set_keep_aspect_ratio(true);
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

fn build_row(
    player: &MprisPlayer,
    sender: &ComponentSender<MprisPopover>,
    show_artwork: bool,
) -> PlayerRowWidgets {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    root.add_css_class("mpris-card");

    let (artwork_picture, artwork_fallback, artwork_revision) = if show_artwork {
        let artwork_box = gtk::Overlay::new();
        artwork_box.add_css_class("mpris-card-art");
        artwork_box.set_valign(gtk::Align::Fill);

        let picture = gtk::Picture::new();
        configure_artwork_picture(&picture);
        artwork_box.set_child(Some(&picture));

        let fallback = gtk::Image::from_icon_name("audio-x-generic-symbolic");
        fallback.set_halign(gtk::Align::Center);
        fallback.set_valign(gtk::Align::Center);
        fallback.add_css_class("mpris-card-art-fallback");
        artwork_box.add_overlay(&fallback);

        root.append(&artwork_box);
        (
            Some(picture),
            Some(fallback),
            Some(Rc::new(Cell::new(0))),
        )
    } else {
        (None, None, None)
    };

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_hexpand(true);
    content.add_css_class("mpris-card-content");

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.add_css_class("mpris-card-copy");

    let title = gtk::Label::new(None);
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    title.add_css_class("mpris-card-title");
    text.append(&title);

    let artist = gtk::Label::new(None);
    artist.set_halign(gtk::Align::Start);
    artist.set_xalign(0.0);
    artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
    artist.set_wrap(true);
    artist.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    artist.add_css_class("mpris-card-subtitle");
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

    let progress_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    progress_box.add_css_class("mpris-card-progress");

    let progress_start = gtk::Label::new(None);
    progress_start.set_xalign(0.0);
    progress_box.append(&progress_start);

    let progress_bar = gtk::ProgressBar::new();
    progress_bar.set_hexpand(true);
    progress_bar.set_valign(gtk::Align::Center);
    progress_box.append(&progress_bar);

    let progress_end = gtk::Label::new(None);
    progress_end.set_xalign(1.0);
    progress_box.append(&progress_end);

    content.append(&progress_box);
    root.append(&content);

    let mut widgets = PlayerRowWidgets {
        root,
        title,
        artist,
        previous,
        play_pause,
        next,
        open,
        progress_box,
        progress_start,
        progress_bar,
        progress_end,
        artwork_picture,
        artwork_fallback,
        artwork_revision,
        current_artwork: MprisArtwork::None,
    };
    update_row(&mut widgets, player);
    widgets
}

fn update_row(widgets: &mut PlayerRowWidgets, player: &MprisPlayer) {
    widgets.title.set_label(&player_title(player));
    widgets.artist.set_label(&player.subtitle);
    widgets.artist.set_visible(!player.subtitle.is_empty());

    widgets.previous.set_sensitive(player.can_go_previous);
    widgets.play_pause.set_sensitive(player.can_play_pause);
    widgets
        .play_pause
        .set_icon_name(play_pause_icon(player.playback_status));
    widgets.next.set_sensitive(player.can_go_next);
    widgets.open.set_sensitive(player.can_raise);

    let progress_visible = shows_progress(player);
    widgets.progress_box.set_visible(progress_visible);
    if progress_visible {
        let position = player.position.unwrap_or(0);
        let length = player.length.unwrap_or(0);
        widgets.progress_start.set_label(&format_duration(position));
        widgets.progress_bar.set_fraction(progress_fraction(position, length));
        widgets.progress_end.set_label(&format_duration(length));
    }

    if let (Some(picture), Some(fallback), Some(revision)) = (
        widgets.artwork_picture.as_ref(),
        widgets.artwork_fallback.as_ref(),
        widgets.artwork_revision.as_ref(),
    ) && artwork_needs_reload(&widgets.current_artwork, &player.artwork)
    {
        load_player_art(picture, fallback, &player.artwork, revision);
        widgets.current_artwork = player.artwork.clone();
    }
}

impl MprisPopover {
    fn sync_rows(&mut self, players: Vec<MprisPlayer>, sender: &ComponentSender<Self>) {
        let players = visible_players(players, self.max_rows);
        self.empty_label.set_visible(players.is_empty());

        let next_ids = players
            .iter()
            .map(|player| player.player_id.as_str())
            .collect::<Vec<_>>();
        let to_remove = self
            .rows
            .keys()
            .filter(|player_id| !next_ids.contains(&player_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        for player_id in to_remove {
            if let Some(row) = self.rows.remove(&player_id) {
                self.rows_box.remove(&row.root);
            }
        }

        for player in &players {
            if let Some(row) = self.rows.get_mut(&player.player_id) {
                update_row(row, player);
            } else {
                let row = build_row(player, sender, self.show_artwork);
                self.rows_box.append(&row.root);
                self.rows.insert(player.player_id.clone(), row);
            }
        }

        let mut previous: Option<gtk::Widget> = None;
        for player in &players {
            let Some(row) = self.rows.get(&player.player_id) else {
                continue;
            };
            self.rows_box.reorder_child_after(&row.root, previous.as_ref());
            previous = Some(row.root.clone().upcast());
        }
    }
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
            rows: HashMap::new(),
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
                self.sync_rows(players, &sender);
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

    #[test]
    fn artwork_reload_only_happens_when_source_changes() {
        let current = MprisArtwork::RemoteUrl("https://example.com/cover.jpg".into());
        assert!(!artwork_needs_reload(&current, &current));
        assert!(artwork_needs_reload(
            &current,
            &MprisArtwork::FileUri("file:///tmp/cover.jpg".into())
        ));
    }
}
