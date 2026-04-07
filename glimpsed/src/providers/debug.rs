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
                        }).await.is_err() { break; }
                        if events.send(ProviderEvent {
                            topic: "debug.timestamp".into(),
                            data: json!(chrono::Utc::now().to_rfc3339()),
                        }).await.is_err() { break; }
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
    use glimpse_types::{Request, RequestBody, RequestResult, Response, ResponseBody};

    #[tokio::test]
    async fn subscribe_receives_counter_and_timestamp() {
        let (broker, tx) = Broker::new(vec![Box::new(DebugProviderFactory)]);
        tokio::spawn(broker.run());

        let (client_tx, mut rx) = mpsc::channel(32);
        tx.send(BrokerMsg::ClientConnected {
            id: 1,
            tx: client_tx,
        })
        .await
        .unwrap();

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request {
                id: 1,
                body: RequestBody::Subscribe {
                    pattern: "debug.**".into(),
                },
            },
        })
        .await
        .unwrap();

        let resp = rx.recv().await.unwrap();
        assert!(matches!(
            resp.body,
            ResponseBody::SubscribeAck {
                available: true,
                ..
            }
        ));

        let mut got_counter = false;
        let mut got_timestamp = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        while tokio::time::Instant::now() < deadline {
            let resp = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("timed out")
                .unwrap();
            if let ResponseBody::Event { topic, .. } = &resp.body {
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
        assert!(got_counter, "never received debug.counter");
        assert!(got_timestamp, "never received debug.timestamp");
    }

    #[tokio::test]
    async fn get_snapshot() {
        let (broker, tx) = Broker::new(vec![Box::new(DebugProviderFactory)]);
        tokio::spawn(broker.run());

        let (client_tx, mut rx) = mpsc::channel(32);
        tx.send(BrokerMsg::ClientConnected {
            id: 1,
            tx: client_tx,
        })
        .await
        .unwrap();

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request {
                id: 1,
                body: RequestBody::Subscribe {
                    pattern: "debug.**".into(),
                },
            },
        })
        .await
        .unwrap();
        let _ = rx.recv().await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request {
                id: 2,
                body: RequestBody::Get {
                    topic: "debug.counter".into(),
                },
            },
        })
        .await
        .unwrap();

        loop {
            let resp = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("timed out")
                .unwrap();
            if let ResponseBody::GetResult { result, .. } = &resp.body {
                assert_eq!(resp.id, 2);
                assert!(matches!(result, RequestResult::Ok { .. }));
                break;
            }
        }
    }

    #[tokio::test]
    async fn call_echo() {
        let (broker, tx) = Broker::new(vec![Box::new(DebugProviderFactory)]);
        tokio::spawn(broker.run());

        let (client_tx, mut rx) = mpsc::channel(32);
        tx.send(BrokerMsg::ClientConnected {
            id: 1,
            tx: client_tx,
        })
        .await
        .unwrap();

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request {
                id: 1,
                body: RequestBody::Subscribe {
                    pattern: "debug.**".into(),
                },
            },
        })
        .await
        .unwrap();
        let _ = rx.recv().await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request {
                id: 3,
                body: RequestBody::Call {
                    method: "debug.echo".into(),
                    params: json!({"msg": "hello"}),
                },
            },
        })
        .await
        .unwrap();

        loop {
            let resp = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("timed out")
                .unwrap();
            if let ResponseBody::CallResult { result, .. } = &resp.body {
                assert_eq!(resp.id, 3);
                if let RequestResult::Ok { data } = result {
                    assert_eq!(*data, json!({"msg": "hello"}));
                } else {
                    panic!("expected Ok");
                }
                break;
            }
        }
    }
}
