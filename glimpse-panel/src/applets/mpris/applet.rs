use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};
use serde::Deserialize;

use super::config::MprisConfig;
use super::popover::{MprisPopover, MprisPopoverInit, MprisPopoverInput, PlayerRow};

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct CurrentPlayer {
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

pub struct Mpris {
    config: MprisConfig,
    current: Option<CurrentPlayer>,
    label: String,
    hidden: bool,
    popover: Controller<MprisPopover>,
}

pub struct MprisInit {
    pub config: MprisConfig,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum MprisMsg {
    CurrentUpdate(CurrentPlayer),
    PlayersUpdate(Vec<PlayerRow>),
    ClearCurrent,
    TogglePopover,
    Unavailable,
}

fn panel_label(player: &CurrentPlayer, format: &str) -> String {
    let label = format
        .replace("{artist}", &player.artist)
        .replace("{track}", &player.track)
        .trim_matches([' ', '-', '—'])
        .trim()
        .to_string();

    if !label.is_empty() {
        label
    } else if !player.track.is_empty() {
        player.track.clone()
    } else {
        player.identity.clone()
    }
}

#[relm4::component(pub)]
impl Component for Mpris {
    type Init = MprisInit;
    type Input = MprisMsg;
    type Output = ();
    type CommandOutput = MprisMsg;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 4,
            add_css_class: "applet",
            add_css_class: "mpris",

            #[watch]
            set_visible: !model.hidden,

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(MprisMsg::TogglePopover);
                }
            },

            gtk::Label {
                #[watch]
                set_label: &model.label,
                #[watch]
                set_visible: !model.label.is_empty(),
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let popover = MprisPopover::builder()
            .launch(MprisPopoverInit {
                parent: root.clone(),
                client: init.client.clone(),
                max_rows: init.config.max_rows,
            })
            .detach();

        let model = Mpris {
            config: init.config,
            current: None,
            label: String::new(),
            hidden: true,
            popover,
        };

        let client = init.client;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("mpris applet: subscribing");
                    let mut current_sub = match client.subscribe("mpris.current").await {
                        Ok(subscription) => subscription,
                        Err(error) => {
                            tracing::error!("mpris: subscribe failed: {error}");
                            let _ = out.send(MprisMsg::Unavailable);
                            return;
                        }
                    };
                    let mut players_sub = client.subscribe("mpris.players").await.ok();

                    loop {
                        tokio::select! {
                            Some(event) = current_sub.next() => {
                                if event.data.is_null() {
                                    let _ = out.send(MprisMsg::ClearCurrent);
                                } else if let Ok(player) =
                                    serde_json::from_value::<CurrentPlayer>(event.data)
                                {
                                    let _ = out.send(MprisMsg::CurrentUpdate(player));
                                } else {
                                    tracing::warn!("mpris: invalid current payload");
                                }
                            }
                            Some(event) = async {
                                match &mut players_sub {
                                    Some(sub) => sub.next().await,
                                    None => std::future::pending().await,
                                }
                            } => {
                                match serde_json::from_value::<Vec<PlayerRow>>(event.data) {
                                    Ok(players) => {
                                        let _ = out.send(MprisMsg::PlayersUpdate(players));
                                    }
                                    Err(error) => tracing::warn!(%error, "mpris: invalid players payload"),
                                }
                            }
                            else => {
                                let _ = out.send(MprisMsg::Unavailable);
                                break;
                            }
                        }
                    }
                })
                .drop_on_shutdown()
        });

        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update_cmd(
        &mut self,
        message: Self::CommandOutput,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        self.update(message, sender, root);
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            MprisMsg::CurrentUpdate(player) => {
                self.label = panel_label(&player, &self.config.label_format);
                self.hidden = false;
                self.current = Some(player);
            }
            MprisMsg::PlayersUpdate(players) => {
                self.popover.emit(MprisPopoverInput::UpdatePlayers(players));
            }
            MprisMsg::ClearCurrent | MprisMsg::Unavailable => {
                self.current = None;
                self.label.clear();
                self.hidden = self.config.hide_when_empty;
                self.popover.emit(MprisPopoverInput::UpdatePlayers(Vec::new()));
            }
            MprisMsg::TogglePopover => {
                self.popover.emit(MprisPopoverInput::Toggle);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_artist_and_track() {
        let player = CurrentPlayer {
            artist: "Nils Frahm".into(),
            track: "Says".into(),
            identity: "Spotify".into(),
            ..CurrentPlayer::default()
        };

        assert_eq!(panel_label(&player, "{artist} - {track}"), "Nils Frahm - Says");
    }

    #[test]
    fn falls_back_to_track_when_format_renders_empty() {
        let player = CurrentPlayer {
            track: "Says".into(),
            identity: "Spotify".into(),
            ..CurrentPlayer::default()
        };

        assert_eq!(panel_label(&player, "{artist}"), "Says");
    }

    #[test]
    fn falls_back_to_identity_when_metadata_is_missing() {
        let player = CurrentPlayer {
            identity: "Firefox".into(),
            ..CurrentPlayer::default()
        };

        assert_eq!(panel_label(&player, "{artist} - {track}"), "Firefox");
    }
}
