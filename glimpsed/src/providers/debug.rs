use std::pin::Pin;
use std::time::Duration;

use serde_json::json;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::provider::{Provider, ProviderEvent, ProviderFactory, ProviderRequest};

const NAME: &str = "debug";
const TOPICS: &[&str] = &["debug.counter", "debug.timestamp"];
const METHODS: &[&str] = &["debug.echo"];

struct DebugProvider {
    counter: u64,
}

impl Provider for DebugProvider {
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
        events: mpsc::Sender<ProviderEvent>,
        mut requests: mpsc::Receiver<ProviderRequest>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
        Box::pin(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    req = requests.recv() => {
                        let Some(req) = req else { break };
                        self.handle_request(req);
                    }
                    _ = interval.tick() => {
                        self.counter += 1;
                        if events.send(ProviderEvent {
                            topic: "debug.counter".into(),
                            data: json!(self.counter),
                        }).await.is_err() {
                            break;
                        }
                        if events.send(ProviderEvent {
                            topic: "debug.timestamp".into(),
                            data: json!(chrono::Utc::now().to_rfc3339()),
                        }).await.is_err() {
                            break;
                        }
                    }
                }
            }
            Ok(())
        })
    }
}

impl DebugProvider {
    fn handle_request(&self, req: ProviderRequest) {
        match req {
            ProviderRequest::Snapshot { topic, reply } => {
                let data = match topic.as_str() {
                    "debug.counter" => Some(json!(self.counter)),
                    "debug.timestamp" => Some(json!(chrono::Utc::now().to_rfc3339())),
                    _ => None,
                };
                let _ = reply.send(data);
            }
            ProviderRequest::Call {
                method,
                params,
                reply,
            } => {
                let result = match method.as_str() {
                    "debug.echo" => Ok(params),
                    _ => Err(anyhow::anyhow!("unknown method: {method}")),
                };
                let _ = reply.send(result);
            }
        }
    }
}

pub struct DebugProviderFactory;

impl ProviderFactory for DebugProviderFactory {
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
        Box::new(DebugProvider { counter: 0 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{Broker, BrokerMsg};
    use glimpse_types::{Request, RequestResult, Response};

    #[tokio::test]
    async fn subscribe_receives_counter_and_timestamp() {
        let (broker, tx) = Broker::new(vec![Box::new(DebugProviderFactory)]);
        tokio::spawn(broker.run());

        let (client_tx, mut client_rx) = mpsc::channel(32);
        tx.send(BrokerMsg::ClientConnected {
            id: 1,
            tx: client_tx,
        })
        .await
        .unwrap();

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Subscribe {
                pattern: "debug.**".into(),
            },
        })
        .await
        .unwrap();

        // Ack.
        let resp = client_rx.recv().await.unwrap();
        assert!(matches!(
            resp,
            Response::SubscribeAck {
                available: true,
                ..
            }
        ));

        // Should receive both counter and timestamp events within 2 seconds.
        let mut got_counter = false;
        let mut got_timestamp = false;

        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while tokio::time::Instant::now() < deadline {
            let resp =
                tokio::time::timeout(Duration::from_secs(2), client_rx.recv())
                    .await
                    .expect("timed out")
                    .unwrap();
            if let Response::Event { topic, .. } = &resp {
                match topic.as_str() {
                    "debug.counter" => got_counter = true,
                    "debug.timestamp" => got_timestamp = true,
                    _ => {}
                }
            }
            if got_counter && got_timestamp {
                break;
            }
        }

        assert!(got_counter, "never received debug.counter event");
        assert!(got_timestamp, "never received debug.timestamp event");
    }

    #[tokio::test]
    async fn get_snapshot() {
        let (broker, tx) = Broker::new(vec![Box::new(DebugProviderFactory)]);
        tokio::spawn(broker.run());

        let (client_tx, mut client_rx) = mpsc::channel(32);
        tx.send(BrokerMsg::ClientConnected {
            id: 1,
            tx: client_tx,
        })
        .await
        .unwrap();

        // Subscribe to start the provider.
        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Subscribe {
                pattern: "debug.**".into(),
            },
        })
        .await
        .unwrap();
        let _ = client_rx.recv().await; // ack
        tokio::time::sleep(Duration::from_millis(100)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Get {
                topic: "debug.counter".into(),
            },
        })
        .await
        .unwrap();

        // Drain until GetResult.
        loop {
            let resp = tokio::time::timeout(Duration::from_secs(2), client_rx.recv())
                .await
                .expect("timed out")
                .unwrap();
            if let Response::GetResult { topic, result } = resp {
                assert_eq!(topic, "debug.counter");
                assert!(matches!(result, RequestResult::Ok { .. }));
                break;
            }
        }
    }

    #[tokio::test]
    async fn call_echo() {
        let (broker, tx) = Broker::new(vec![Box::new(DebugProviderFactory)]);
        tokio::spawn(broker.run());

        let (client_tx, mut client_rx) = mpsc::channel(32);
        tx.send(BrokerMsg::ClientConnected {
            id: 1,
            tx: client_tx,
        })
        .await
        .unwrap();

        // Subscribe to start the provider.
        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Subscribe {
                pattern: "debug.**".into(),
            },
        })
        .await
        .unwrap();
        let _ = client_rx.recv().await; // ack
        tokio::time::sleep(Duration::from_millis(100)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Call {
                method: "debug.echo".into(),
                params: json!({"msg": "hello"}),
            },
        })
        .await
        .unwrap();

        // Drain until CallResult.
        loop {
            let resp = tokio::time::timeout(Duration::from_secs(2), client_rx.recv())
                .await
                .expect("timed out")
                .unwrap();
            if let Response::CallResult { method, result } = resp {
                assert_eq!(method, "debug.echo");
                if let RequestResult::Ok { data } = result {
                    assert_eq!(data, json!({"msg": "hello"}));
                } else {
                    panic!("expected Ok, got error");
                }
                break;
            }
        }
    }
}
