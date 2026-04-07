use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
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

fn build_row(player: &PlayerRow, client: &Arc<Client>) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    root.add_css_class("mpris-row");
    root.set_valign(gtk::Align::Center);

    let art = gtk::Image::from_icon_name("audio-x-generic-symbolic");
    art.set_pixel_size(32);
    art.set_valign(gtk::Align::Center);
    art.add_css_class("mpris-art");
    root.append(&art);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(Some(&player_title(player)));
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.add_css_class("mpris-track");
    text.append(&title);

    let subtitle = gtk::Label::new(Some(&row_subtitle(player)));
    subtitle.set_halign(gtk::Align::Start);
    subtitle.set_xalign(0.0);
    subtitle.add_css_class("mpris-artist");
    text.append(&subtitle);

    root.append(&text);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    controls.set_valign(gtk::Align::Center);
    controls.add_css_class("mpris-controls");

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

    root.append(&controls);
    root
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
