use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, gdk, gio, glib, prelude::*},
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct PlayerRow {
    pub player_id: String,
    pub identity: String,
    pub artist: String,
    pub track: String,
    pub album: String,
    pub status: String,
    pub art_url: String,
    pub can_go_previous: bool,
    pub can_play_pause: bool,
    pub can_go_next: bool,
    pub last_active: u64,
}

pub struct MprisPopover {
    popover: gtk::Popover,
    client: Arc<Client>,
    rows_box: gtk::Box,
    empty_label: gtk::Label,
    max_rows: usize,
}

pub struct MprisPopoverInit {
    pub parent: gtk::Box,
    pub client: Arc<Client>,
    pub max_rows: usize,
}

#[derive(Debug)]
pub enum MprisPopoverInput {
    Toggle,
    UpdatePlayers(Vec<PlayerRow>),
}

const CARD_ART_SIZE: i32 = 168;

#[derive(Debug, Clone, PartialEq, Eq)]
enum ArtSource {
    FilePath(String),
    FileUri(String),
    Remote(String),
    Fallback,
}

fn spawn_call(client: &Arc<Client>, method: &'static str, player_id: String) {
    let client = client.clone();
    glib::spawn_future_local(async move {
        let _ = client
            .call(method, serde_json::json!({ "player_id": player_id }))
            .await;
    });
}

fn row_subtitle(player: &PlayerRow) -> String {
    if !player.artist.is_empty() {
        player.artist.clone()
    } else if !player.album.is_empty() {
        player.album.clone()
    } else {
        player.identity.clone()
    }
}

fn parse_art_source(value: &str) -> ArtSource {
    if value.starts_with("file://") {
        ArtSource::FileUri(value.to_string())
    } else if value.starts_with("http://") || value.starts_with("https://") {
        ArtSource::Remote(value.to_string())
    } else if value.starts_with('/') {
        ArtSource::FilePath(value.to_string())
    } else {
        ArtSource::Fallback
    }
}

fn sorted_players(mut players: Vec<PlayerRow>, max_rows: usize) -> Vec<PlayerRow> {
    players.sort_by(|a, b| {
        b.last_active
            .cmp(&a.last_active)
            .then_with(|| a.player_id.cmp(&b.player_id))
    });
    players.truncate(max_rows);
    players
}

fn play_pause_icon(status: &str) -> &'static str {
    if status == "Playing" {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    }
}

fn player_title(player: &PlayerRow) -> String {
    if !player.track.is_empty() {
        player.track.clone()
    } else {
        player.identity.clone()
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

fn set_fallback_art(widget: &gtk::Image) {
    widget.set_icon_name(Some("audio-x-generic-symbolic"));
}

fn load_player_art(widget: &gtk::Image, art_url: &str) {
    match parse_art_source(art_url) {
        ArtSource::FilePath(path) => {
            let file = gio::File::for_path(path);
            match gdk::Texture::from_file(&file) {
                Ok(texture) => widget.set_paintable(Some(&texture)),
                Err(_) => set_fallback_art(widget),
            }
        }
        ArtSource::FileUri(uri) => {
            let file = gio::File::for_uri(&uri);
            match gdk::Texture::from_file(&file) {
                Ok(texture) => widget.set_paintable(Some(&texture)),
                Err(_) => set_fallback_art(widget),
            }
        }
        ArtSource::Remote(url) => {
            let widget = widget.clone();
            glib::spawn_future_local(async move {
                let Ok(response) = reqwest::get(&url).await else {
                    set_fallback_art(&widget);
                    return;
                };
                let Ok(bytes) = response.bytes().await else {
                    set_fallback_art(&widget);
                    return;
                };
                match gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes.to_vec())) {
                    Ok(texture) => widget.set_paintable(Some(&texture)),
                    Err(_) => set_fallback_art(&widget),
                }
            });
        }
        ArtSource::Fallback => set_fallback_art(widget),
    }
}

fn build_row(player: &PlayerRow, client: &Arc<Client>) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("mpris-card");

    let shell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    shell.set_valign(gtk::Align::Fill);

    let art = gtk::Image::from_icon_name("audio-x-generic-symbolic");
    art.set_pixel_size(CARD_ART_SIZE);
    art.set_size_request(CARD_ART_SIZE, CARD_ART_SIZE);
    art.set_halign(gtk::Align::Fill);
    art.set_valign(gtk::Align::Fill);
    art.add_css_class("mpris-card-art");
    load_player_art(&art, &player.art_url);
    shell.append(&art);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_hexpand(true);
    content.set_valign(gtk::Align::Fill);
    content.add_css_class("mpris-card-content");

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Start);
    text.add_css_class("mpris-card-copy");

    let title = gtk::Label::new(Some(&player_title(player)));
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(22);
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    title.add_css_class("mpris-card-title");
    text.append(&title);

    let subtitle = gtk::Label::new(Some(&row_subtitle(player)));
    subtitle.set_halign(gtk::Align::Start);
    subtitle.set_xalign(0.0);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_max_width_chars(24);
    subtitle.set_wrap(true);
    subtitle.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    subtitle.add_css_class("mpris-card-subtitle");
    text.append(&subtitle);

    content.append(&text);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    controls.set_halign(gtk::Align::Start);
    controls.set_valign(gtk::Align::End);
    controls.add_css_class("mpris-card-controls");

    let player_id = player.player_id.clone();
    let prev = media_button(
        "media-skip-backward-symbolic",
        player.can_go_previous,
        {
            let client = client.clone();
            let player_id = player_id.clone();
            move || spawn_call(&client, "mpris.previous", player_id.clone())
        },
    );
    controls.append(&prev);

    let play_pause = media_button(play_pause_icon(&player.status), player.can_play_pause, {
        let client = client.clone();
        let player_id = player_id.clone();
        move || spawn_call(&client, "mpris.play_pause", player_id.clone())
    });
    controls.append(&play_pause);

    let next = media_button("media-skip-forward-symbolic", player.can_go_next, {
        let client = client.clone();
        let player_id = player.player_id.clone();
        move || spawn_call(&client, "mpris.next", player_id.clone())
    });
    controls.append(&next);

    content.append(&controls);
    shell.append(&content);
    card.append(&shell);
    card
}

impl SimpleComponent for MprisPopover {
    type Init = MprisPopoverInit;
    type Input = MprisPopoverInput;
    type Output = ();
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
            client: init.client,
            rows_box,
            empty_label,
            max_rows: init.max_rows,
        };
        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            MprisPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            MprisPopoverInput::UpdatePlayers(players) => {
                let players = sorted_players(players, self.max_rows);
                self.empty_label.set_visible(players.is_empty());

                while let Some(child) = self.rows_box.first_child() {
                    self.rows_box.remove(&child);
                }

                for player in &players {
                    self.rows_box.append(&build_row(player, &self.client));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_art_size_matches_large_media_layout() {
        assert_eq!(CARD_ART_SIZE, 168);
    }

    #[test]
    fn artwork_source_parses_file_urls() {
        assert_eq!(
            parse_art_source("file:///tmp/cover.jpg"),
            ArtSource::FileUri("file:///tmp/cover.jpg".into())
        );
    }

    #[test]
    fn artwork_source_parses_https_urls() {
        assert_eq!(
            parse_art_source("https://example.com/cover.jpg"),
            ArtSource::Remote("https://example.com/cover.jpg".into())
        );
    }

    #[test]
    fn artwork_source_falls_back_for_unknown_values() {
        assert_eq!(parse_art_source(""), ArtSource::Fallback);
    }

    #[test]
    fn subtitle_falls_back_to_album_then_identity() {
        let player = PlayerRow {
            album: "Promises".into(),
            identity: "Spotify".into(),
            ..PlayerRow::default()
        };

        assert_eq!(row_subtitle(&player), "Promises");
    }

    #[test]
    fn sort_players_newest_first() {
        let players = vec![
            PlayerRow {
                player_id: "a".into(),
                last_active: 1,
                ..PlayerRow::default()
            },
            PlayerRow {
                player_id: "b".into(),
                last_active: 9,
                ..PlayerRow::default()
            },
        ];

        let sorted = sorted_players(players, 6);
        assert_eq!(sorted[0].player_id, "b");
    }
}
