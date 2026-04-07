use std::pin::Pin;

use serde::Serialize;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "mpris";
const TOPICS: &[&str] = &["mpris.current", "mpris.players"];
const METHODS: &[&str] = &[
    "mpris.play_pause",
    "mpris.previous",
    "mpris.next",
    "mpris.raise",
];

#[derive(Debug, Clone, Serialize, Default)]
struct PlayerSnapshot {
    player_id: String,
    bus_name: String,
    identity: String,
    artist: String,
    track: String,
    album: String,
    status: String,
    art_url: String,
    can_go_previous: bool,
    can_play_pause: bool,
    can_go_next: bool,
    last_active: u64,
}

impl PlayerSnapshot {
    #[cfg(test)]
    fn test_default() -> Self {
        Self {
            player_id: String::new(),
            bus_name: String::new(),
            identity: String::new(),
            artist: String::new(),
            track: String::new(),
            album: String::new(),
            status: "Stopped".into(),
            art_url: String::new(),
            can_go_previous: false,
            can_play_pause: true,
            can_go_next: false,
            last_active: 0,
        }
    }
}

fn select_current_player(players: &[PlayerSnapshot]) -> Option<PlayerSnapshot> {
    players
        .iter()
        .max_by_key(|player| player.last_active)
        .cloned()
}

#[derive(Debug, Default)]
struct ProviderState {
    players: Vec<PlayerSnapshot>,
}

impl ProviderState {
    fn upsert(&mut self, player: PlayerSnapshot) {
        if let Some(existing) = self
            .players
            .iter_mut()
            .find(|existing| existing.player_id == player.player_id)
        {
            *existing = player;
        } else {
            self.players.push(player);
        }

        self.players.sort_by(|a, b| {
            b.last_active
                .cmp(&a.last_active)
                .then_with(|| a.player_id.cmp(&b.player_id))
        });
    }

    fn remove(&mut self, player_id: &str) {
        self.players.retain(|player| player.player_id != player_id);
    }

    fn current(&self) -> Option<PlayerSnapshot> {
        select_current_player(&self.players)
    }
}

impl PlayerSnapshot {
    fn subtitle(&self) -> String {
        if !self.artist.is_empty() {
            self.artist.clone()
        } else if !self.album.is_empty() {
            self.album.clone()
        } else {
            self.identity.clone()
        }
    }

    fn panel_label(&self, format: &str) -> String {
        let rendered = format
            .replace("{artist}", &self.artist)
            .replace("{track}", &self.track)
            .trim_matches([' ', '-', '—'])
            .trim()
            .to_string();

        if !rendered.is_empty() {
            rendered
        } else if !self.track.is_empty() {
            self.track.clone()
        } else {
            self.identity.clone()
        }
    }
}

#[derive(Debug, Default)]
pub struct MprisProvider {
    state: ProviderState,
}

impl Provider for MprisProvider {
    fn name(&self) -> &'static str {
        NAME
    }

    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }

    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }

    fn run(
        &mut self,
        _events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req).await;
                    }
                }
            }

            Ok(())
        })
    }
}

impl MprisProvider {
    async fn handle_request(&self, req: ProviderRequest) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "mpris.current" => serde_json::to_value(self.state.current()).ok(),
                    "mpris.players" => serde_json::to_value(&self.state.players).ok(),
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call { reply, .. } => {
                let _ = reply.send(Err(anyhow::anyhow!("not implemented")));
            }
        }
    }
}

pub struct MprisProviderFactory;

impl ProviderFactory for MprisProviderFactory {
    fn name(&self) -> &'static str {
        NAME
    }

    fn topics(&self) -> &'static [&'static str] {
        TOPICS
    }

    fn methods(&self) -> &'static [&'static str] {
        METHODS
    }

    fn create(&self) -> Box<dyn Provider> {
        Box::new(MprisProvider::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_metadata() {
        assert_eq!(NAME, "mpris");
        assert_eq!(TOPICS, &["mpris.current", "mpris.players"]);
        assert_eq!(
            METHODS,
            &[
                "mpris.play_pause",
                "mpris.previous",
                "mpris.next",
                "mpris.raise"
            ]
        );
    }

    #[test]
    fn selects_newest_player_for_current() {
        let players = vec![
            PlayerSnapshot {
                player_id: "spotify".into(),
                last_active: 10,
                ..PlayerSnapshot::test_default()
            },
            PlayerSnapshot {
                player_id: "firefox".into(),
                last_active: 20,
                ..PlayerSnapshot::test_default()
            },
        ];

        let current = select_current_player(&players).expect("current player");
        assert_eq!(current.player_id, "firefox");
    }

    #[tokio::test]
    async fn current_snapshot_returns_null() {
        let provider = MprisProvider::default();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        provider
            .handle_request(ProviderRequest::Snapshot {
                topic: "mpris.current".into(),
                reply: reply_tx,
            })
            .await;

        assert_eq!(reply_rx.await.unwrap(), Some(serde_json::Value::Null));
    }

    #[tokio::test]
    async fn players_snapshot_returns_empty_array() {
        let provider = MprisProvider::default();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        provider
            .handle_request(ProviderRequest::Snapshot {
                topic: "mpris.players".into(),
                reply: reply_tx,
            })
            .await;

        assert_eq!(reply_rx.await.unwrap(), Some(serde_json::json!([])));
    }

    #[tokio::test]
    async fn methods_return_not_implemented() {
        let provider = MprisProvider::default();
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        provider
            .handle_request(ProviderRequest::Call {
                method: "mpris.play_pause".into(),
                params: serde_json::json!({}),
                reply: reply_tx,
            })
            .await;

        let err = reply_rx.await.unwrap().expect_err("expected error");
        assert_eq!(err.to_string(), "not implemented");
    }

    #[test]
    fn subtitle_falls_back_from_artist_and_album_to_identity() {
        let player = PlayerSnapshot {
            identity: "Firefox".into(),
            artist: String::new(),
            album: String::new(),
            ..PlayerSnapshot::test_default()
        };

        assert_eq!(player.subtitle(), "Firefox");
    }

    #[test]
    fn panel_label_falls_back_from_missing_artist_and_track() {
        let player = PlayerSnapshot {
            identity: "Firefox".into(),
            artist: String::new(),
            track: String::new(),
            ..PlayerSnapshot::test_default()
        };

        assert_eq!(player.panel_label("{artist} - {track}"), "Firefox");
    }

    #[test]
    fn provider_state_orders_and_replaces_players() {
        let mut state = ProviderState::default();
        state.upsert(PlayerSnapshot {
            player_id: "spotify".into(),
            last_active: 10,
            ..PlayerSnapshot::test_default()
        });
        state.upsert(PlayerSnapshot {
            player_id: "firefox".into(),
            last_active: 20,
            ..PlayerSnapshot::test_default()
        });
        state.upsert(PlayerSnapshot {
            player_id: "spotify".into(),
            last_active: 30,
            identity: "Spotify".into(),
            ..PlayerSnapshot::test_default()
        });

        assert_eq!(state.players.len(), 2);
        assert_eq!(state.current().unwrap().player_id, "spotify");
        assert_eq!(state.current().unwrap().last_active, 30);
        assert_eq!(state.players[1].player_id, "firefox");
    }

    #[tokio::test]
    async fn snapshot_replies_use_provider_state() {
        let provider = MprisProvider {
            state: ProviderState {
                players: vec![
                    PlayerSnapshot {
                        player_id: "spotify".into(),
                        identity: "Spotify".into(),
                        last_active: 10,
                        ..PlayerSnapshot::test_default()
                    },
                    PlayerSnapshot {
                        player_id: "firefox".into(),
                        identity: "Firefox".into(),
                        last_active: 20,
                        ..PlayerSnapshot::test_default()
                    },
                ],
            },
        };

        let (current_reply_tx, current_reply_rx) = tokio::sync::oneshot::channel();
        provider
            .handle_request(ProviderRequest::Snapshot {
                topic: "mpris.current".into(),
                reply: current_reply_tx,
            })
            .await;

        let current = current_reply_rx.await.unwrap().expect("current snapshot");
        assert_eq!(current["player_id"], "firefox");

        let (players_reply_tx, players_reply_rx) = tokio::sync::oneshot::channel();
        provider
            .handle_request(ProviderRequest::Snapshot {
                topic: "mpris.players".into(),
                reply: players_reply_tx,
            })
            .await;

        let players = players_reply_rx.await.unwrap().expect("players snapshot");
        assert_eq!(players.as_array().unwrap().len(), 2);
        assert_eq!(players[0]["player_id"], "spotify");
        assert_eq!(players[1]["player_id"], "firefox");
    }
}
