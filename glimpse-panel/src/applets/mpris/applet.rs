use glimpse::mpris::{
    MprisServiceHandle,
    protocol::{MprisPlayer, MprisServiceCommand, MprisServiceState},
};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller,
    gtk::{self, prelude::*},
};

use super::config::MprisConfig;
use super::popover::{MprisPopover, MprisPopoverInit, MprisPopoverInput, MprisPopoverOutput};

const PANEL_LABEL_MAX_WIDTH_CHARS: i32 = 48;

pub struct Mpris {
    config: MprisConfig,
    service: MprisServiceHandle,
    label: String,
    tooltip: String,
    hidden: bool,
    popover: Controller<MprisPopover>,
}

pub struct MprisInit {
    pub config: MprisConfig,
    pub service: MprisServiceHandle,
}

#[derive(Debug, Clone)]
pub enum MprisMsg {
    ServiceState(MprisServiceState),
    PopoverOutput(MprisPopoverOutput),
    TogglePopover,
    Unavailable,
}

fn command_for_popover_output(output: MprisPopoverOutput) -> MprisServiceCommand {
    match output {
        MprisPopoverOutput::Previous { player_id } => MprisServiceCommand::Previous { player_id },
        MprisPopoverOutput::PlayPause { player_id } => MprisServiceCommand::PlayPause { player_id },
        MprisPopoverOutput::Next { player_id } => MprisServiceCommand::Next { player_id },
        MprisPopoverOutput::Raise { player_id } => MprisServiceCommand::Raise { player_id },
    }
}

fn panel_label(player: &MprisPlayer, format: &str) -> String {
    let label = format
        .replace("{artist}", &player.artist)
        .replace("{track}", &player.title)
        .replace("{title}", &player.title)
        .trim_matches([' ', '-', '—'])
        .trim()
        .to_string();

    if !label.is_empty() {
        label
    } else if !player.panel_label.is_empty() {
        player.panel_label.clone()
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
            set_hexpand: false,
            add_css_class: "applet",
            add_css_class: "mpris",
            add_css_class: "hoverable",
            #[watch]
            set_visible: !model.hidden,
            #[watch]
            set_tooltip_text: if model.tooltip.is_empty() { None } else { Some(&model.tooltip) },

            add_controller = gtk::GestureClick {
                set_button: 1,
                connect_pressed[sender] => move |_, _, _, _| {
                    sender.input(MprisMsg::TogglePopover);
                }
            },

            gtk::Label {
                #[watch]
                set_label: &model.label,
                set_hexpand: false,
                set_max_width_chars: PANEL_LABEL_MAX_WIDTH_CHARS,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Center,
                set_xalign: 0.0,
                set_wrap: false,
                set_single_line_mode: true,
                set_ellipsize: gtk::pango::EllipsizeMode::End,
                add_css_class: "mpris-label",
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
                max_rows: init.config.max_rows,
                show_artwork: init.config.show_artwork,
            })
            .forward(sender.input_sender(), MprisMsg::PopoverOutput);

        let model = Mpris {
            config: init.config,
            service: init.service.clone(),
            label: String::new(),
            tooltip: String::new(),
            hidden: true,
            popover,
        };

        let service = init.service;
        sender.command(move |out, shutdown| {
            shutdown
                .register(async move {
                    tracing::info!("mpris applet: subscribing to mpris service");
                    let mut state_rx = service.subscribe();
                    let _ = out.send(MprisMsg::ServiceState(state_rx.borrow().clone()));

                    loop {
                        if state_rx.changed().await.is_err() {
                            break;
                        }
                        let _ = out.send(MprisMsg::ServiceState(state_rx.borrow().clone()));
                    }

                    tracing::warn!("mpris applet: mpris service state channel closed");
                    let _ = out.send(MprisMsg::Unavailable);
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

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match message {
            MprisMsg::ServiceState(state) => {
                self.sync_from_state(&state);
                self.popover
                    .emit(MprisPopoverInput::UpdatePlayers(state.snapshot.players));
            }
            MprisMsg::PopoverOutput(output) => {
                self.handle_popover_output(output, sender);
            }
            MprisMsg::TogglePopover => {
                self.popover.emit(MprisPopoverInput::Toggle);
            }
            MprisMsg::Unavailable => {
                tracing::warn!("mpris applet: mpris service unavailable");
                self.label.clear();
                self.tooltip.clear();
                self.hidden = self.config.hide_when_empty;
                self.popover
                    .emit(MprisPopoverInput::UpdatePlayers(Vec::new()));
            }
        }
    }
}

impl Mpris {
    fn sync_from_state(&mut self, state: &MprisServiceState) {
        self.label = state
            .snapshot
            .current_player
            .as_ref()
            .map(|player| panel_label(player, &self.config.label_format))
            .unwrap_or_default();
        self.tooltip = self.label.clone();
        self.hidden = self.config.hide_when_empty && state.snapshot.players.is_empty();
    }

    fn handle_popover_output(&self, output: MprisPopoverOutput, sender: ComponentSender<Self>) {
        self.send_command(sender, command_for_popover_output(output));
    }

    fn send_command(&self, sender: ComponentSender<Self>, command: MprisServiceCommand) {
        let service = self.service.clone();
        sender.command(move |_out, shutdown| {
            shutdown
                .register(async move {
                    if let Err(error) = service.send(command).await {
                        tracing::warn!(error = %error, "mpris applet: failed to send mpris service command");
                    }
                })
                .drop_on_shutdown()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glimpse::mpris::protocol::{MprisArtwork, MprisPlaybackStatus};

    fn player() -> MprisPlayer {
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
    fn formats_artist_and_track() {
        assert_eq!(
            panel_label(&player(), "{artist} - {track}"),
            "Nils Frahm - Says"
        );
    }

    #[test]
    fn falls_back_to_panel_label_when_format_renders_empty() {
        assert_eq!(panel_label(&player(), ""), "Nils Frahm - Says");
    }

    #[test]
    fn panel_label_width_cap_is_reasonable() {
        assert_eq!(PANEL_LABEL_MAX_WIDTH_CHARS, 48);
    }

    #[test]
    fn falls_back_to_identity_when_metadata_is_missing() {
        let mut player = player();
        player.artist.clear();
        player.title.clear();
        player.panel_label.clear();
        assert_eq!(panel_label(&player, "{artist} - {track}"), "Spotify");
    }

    #[test]
    fn maps_popover_previous_output_to_previous_command() {
        let command = command_for_popover_output(MprisPopoverOutput::Previous {
            player_id: "spotify".into(),
        });

        assert_eq!(
            command,
            MprisServiceCommand::Previous {
                player_id: "spotify".into(),
            }
        );
    }

    #[test]
    fn maps_popover_raise_output_to_raise_command() {
        let command = command_for_popover_output(MprisPopoverOutput::Raise {
            player_id: "spotify".into(),
        });

        assert_eq!(
            command,
            MprisServiceCommand::Raise {
                player_id: "spotify".into(),
            }
        );
    }
}
