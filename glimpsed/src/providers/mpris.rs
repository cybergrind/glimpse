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
    players.iter().max_by_key(|player| player.last_active).cloned()
}

pub struct MprisProvider;

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
                    "mpris.current" => Some(serde_json::Value::Null),
                    "mpris.players" => Some(serde_json::json!([])),
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
        Box::new(MprisProvider)
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
            &["mpris.play_pause", "mpris.previous", "mpris.next", "mpris.raise"]
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
        let provider = MprisProvider;
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
        let provider = MprisProvider;
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        provider
            .handle_request(ProviderRequest::Snapshot {
                topic: "mpris.players".into(),
                reply: reply_tx,
            })
            .await;

        assert_eq!(
            reply_rx.await.unwrap(),
            Some(serde_json::json!([]))
        );
    }

    #[tokio::test]
    async fn methods_return_not_implemented() {
        let provider = MprisProvider;
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
}
