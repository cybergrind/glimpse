//  ┌────────┐  NDJSON   ┌────────┐  BrokerMsg   ┌────────┐
//  │ Client │ ───────►  │ Server │ ───────────► │        │
//  │ socket │ ◄───────  │ reader │              │        │
//  └────────┘  Response │ writer │ ◄─────────   │ Broker │
//                       └────────┘  Response    │        │
//                                               │        │
//  ┌──────────┐  ProviderEvent                  │        │
//  │ Provider │ ─────────────────────────────►  │        │
//  │   task   │ ◄──── ProviderRequest ────────  │        │
//  └──────────┘  (snapshot / call via oneshot)  └────────┘

use std::collections::{HashMap, HashSet};

use glimpse_types::{Request, RequestResult, Response};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::pattern::Pattern;
use crate::provider::{ProviderEvent, ProviderFactory, ProviderRequest};
use crate::server::ClientId;

pub enum BrokerMsg {
    ClientConnected {
        id: ClientId,
        tx: mpsc::Sender<Response>,
    },
    ClientDisconnected {
        id: ClientId,
    },
    Request {
        client: ClientId,
        request: Request,
    },
    ProviderEvent(ProviderEvent),
    ProviderStopped {
        name: &'static str,
        error: Option<String>,
    },
}

struct Client {
    tx: mpsc::Sender<Response>,
    subscriptions: HashSet<Pattern>,
}

struct ProviderEntry {
    factory: Box<dyn ProviderFactory>,
    handle: Option<ProviderHandle>,
    topics: &'static [&'static str],
    methods: &'static [&'static str],
}

struct ProviderHandle {
    cancel: CancellationToken,
    task: JoinHandle<()>,
    requests: mpsc::Sender<ProviderRequest>,
}

pub struct Broker {
    rx: mpsc::Receiver<BrokerMsg>,
    tx: mpsc::Sender<BrokerMsg>,
    clients: HashMap<ClientId, Client>,
    providers: HashMap<&'static str, ProviderEntry>,
    /// method name → provider name (index built at init)
    method_index: HashMap<&'static str, &'static str>,
}

impl Broker {
    pub fn new(factories: Vec<Box<dyn ProviderFactory>>) -> (Self, mpsc::Sender<BrokerMsg>) {
        let (tx, rx) = mpsc::channel(256);
        let mut providers = HashMap::new();
        let mut method_index = HashMap::new();

        for f in factories {
            let name = f.name();
            let topics = f.topics();
            let methods = f.methods();
            for &method in methods {
                method_index.insert(method, name);
            }
            providers.insert(
                name,
                ProviderEntry {
                    factory: f,
                    handle: None,
                    topics,
                    methods,
                },
            );
        }

        let broker = Self {
            rx,
            tx: tx.clone(),
            clients: HashMap::new(),
            providers,
            method_index,
        };
        (broker, tx)
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                BrokerMsg::ClientConnected { id, tx } => {
                    self.clients.insert(
                        id,
                        Client {
                            tx,
                            subscriptions: HashSet::new(),
                        },
                    );
                }
                BrokerMsg::ClientDisconnected { id } => {
                    self.clients.remove(&id);
                    self.stop_unused_providers();
                }
                BrokerMsg::Request { client, request } => {
                    self.handle_request(client, request).await;
                }
                BrokerMsg::ProviderEvent(event) => {
                    self.route_event(&event.topic, event.data);
                }
                BrokerMsg::ProviderStopped { name, error } => {
                    if let Some(entry) = self.providers.get_mut(name) {
                        entry.handle = None;
                    }
                    if let Some(err) = &error {
                        tracing::error!(provider = name, "stopped with error: {err}");
                        self.notify_subscribers(name, err);
                    } else {
                        tracing::info!(provider = name, "stopped");
                    }
                }
            }
        }
    }

    async fn handle_request(&mut self, client: ClientId, request: Request) {
        match request {
            Request::Subscribe { pattern } => {
                let pat = Pattern::parse(&pattern);
                let provider_name = pat.provider_name().unwrap_or("");
                let available = self.ensure_provider(provider_name);

                self.send_to(
                    client,
                    Response::SubscribeAck {
                        pattern: pattern.clone(),
                        available,
                        error: if available {
                            None
                        } else {
                            Some(format!("provider '{provider_name}' not found"))
                        },
                    },
                );

                if available {
                    if let Some(entry) = self.providers.get(provider_name) {
                        for &topic in entry.topics {
                            if pat.matches(topic) {
                                if let Some(data) =
                                    self.request_snapshot(provider_name, topic).await
                                {
                                    self.send_to(
                                        client,
                                        Response::Event {
                                            topic: topic.to_owned(),
                                            data,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }

                if let Some(c) = self.clients.get_mut(&client) {
                    c.subscriptions.insert(pat);
                }
            }
            Request::Unsubscribe { pattern } => {
                let pat = Pattern::parse(&pattern);
                if let Some(c) = self.clients.get_mut(&client) {
                    c.subscriptions.remove(&pat);
                }
                self.send_to(client, Response::UnsubscribeAck { pattern });
                self.stop_unused_providers();
            }
            Request::Get { topic } => {
                let provider_name = topic.split('.').next().unwrap_or("").to_owned();
                let available = self.ensure_provider(&provider_name);

                if !available {
                    self.send_to(
                        client,
                        Response::GetResult {
                            topic,
                            result: RequestResult::Error {
                                code: 1,
                                message: format!("provider '{provider_name}' not found"),
                            },
                        },
                    );
                    return;
                }

                let data = self.request_snapshot(&provider_name, &topic).await;
                self.send_to(
                    client,
                    Response::GetResult {
                        topic,
                        result: match data {
                            Some(d) => RequestResult::Ok { data: d },
                            None => RequestResult::Error {
                                code: 2,
                                message: "unknown topic".into(),
                            },
                        },
                    },
                );
            }
            Request::Call { method, params } => {
                let provider_name = self.method_index.get(method.as_str()).copied();
                let Some(name) = provider_name else {
                    self.send_to(
                        client,
                        Response::CallResult {
                            method,
                            result: RequestResult::Error {
                                code: 3,
                                message: "unknown method".into(),
                            },
                        },
                    );
                    return;
                };

                self.ensure_provider(name);
                let result = self.request_call(name, &method, params).await;
                self.send_to(
                    client,
                    Response::CallResult {
                        method,
                        result: match result {
                            Ok(data) => RequestResult::Ok { data },
                            Err(e) => RequestResult::Error {
                                code: 5,
                                message: e.to_string(),
                            },
                        },
                    },
                );
            }
        }
    }

    fn route_event(&self, topic: &str, data: serde_json::Value) {
        for (id, client) in &self.clients {
            if client.subscriptions.iter().any(|p| p.matches(topic)) {
                self.send_to(
                    *id,
                    Response::Event {
                        topic: topic.to_owned(),
                        data: data.clone(),
                    },
                );
            }
        }
    }

    fn send_to(&self, client_id: ClientId, response: Response) {
        if let Some(client) = self.clients.get(&client_id) {
            if client.tx.try_send(response).is_err() {
                tracing::warn!(client = client_id, "client channel full, dropping message");
            }
        }
    }

    fn notify_subscribers(&self, provider_name: &str, error: &str) {
        for (id, client) in &self.clients {
            let matches = client
                .subscriptions
                .iter()
                .any(|p| p.provider_name() == Some(provider_name));
            if matches {
                self.send_to(
                    *id,
                    Response::ProviderUnavailable {
                        provider: provider_name.to_owned(),
                        error: error.to_owned(),
                    },
                );
            }
        }
    }

    async fn request_snapshot(&self, provider: &str, topic: &str) -> Option<serde_json::Value> {
        let entry = self.providers.get(provider)?;
        let handle = entry.handle.as_ref()?;
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .requests
            .send(ProviderRequest::Snapshot {
                topic: topic.to_owned(),
                reply: reply_tx,
            })
            .await
            .ok()?;
        reply_rx.await.ok()?
    }

    async fn request_call(
        &self,
        provider: &str,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let entry = self
            .providers
            .get(provider)
            .ok_or_else(|| anyhow::anyhow!("provider not found"))?;
        let handle = entry
            .handle
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("provider not running"))?;
        let (reply_tx, reply_rx) = oneshot::channel();
        handle
            .requests
            .send(ProviderRequest::Call {
                method: method.to_owned(),
                params,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("provider not responding"))?;
        reply_rx
            .await
            .map_err(|_| anyhow::anyhow!("provider dropped reply"))?
    }

    fn ensure_provider(&mut self, name: &str) -> bool {
        let Some(entry) = self.providers.get(name) else {
            return false;
        };
        if entry.handle.is_some() {
            return true;
        }
        let static_name: &'static str = entry.factory.name();
        let mut provider = entry.factory.create();
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let event_broker_tx = self.tx.clone();
        let stopped_broker_tx = self.tx.clone();
        let (event_tx, mut event_rx) = mpsc::channel::<ProviderEvent>(64);
        let (request_tx, request_rx) = mpsc::channel::<ProviderRequest>(16);

        let task = tokio::spawn(async move {
            let forwarder = tokio::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    if event_broker_tx
                        .send(BrokerMsg::ProviderEvent(event))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });

            let result = provider.run(event_tx, request_rx, cancel_clone).await;
            forwarder.abort();

            let error = result.err().map(|e| e.to_string());
            let _ = stopped_broker_tx
                .send(BrokerMsg::ProviderStopped {
                    name: static_name,
                    error,
                })
                .await;
        });

        // Safe to unwrap — we checked `name` exists above.
        let entry = self.providers.get_mut(name).unwrap();
        entry.handle = Some(ProviderHandle {
            cancel,
            task,
            requests: request_tx,
        });
        tracing::info!(provider = static_name, "started");
        true
    }

    fn stop_unused_providers(&mut self) {
        let in_use: HashSet<&str> = self
            .clients
            .values()
            .flat_map(|c| c.subscriptions.iter().filter_map(|p| p.provider_name()))
            .collect();

        for (name, entry) in &mut self.providers {
            if !in_use.contains(name) {
                if let Some(handle) = entry.handle.take() {
                    handle.cancel.cancel();
                    handle.task.abort();
                    tracing::info!(provider = name, "stopped (no subscribers)");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Provider, ProviderEvent, ProviderRequest};
    use serde_json::json;
    use std::pin::Pin;

    struct TestProvider;

    impl Provider for TestProvider {
        fn name(&self) -> &'static str {
            "test"
        }
        fn topics(&self) -> &'static [&'static str] {
            &["test.value"]
        }
        fn methods(&self) -> &'static [&'static str] {
            &["test.echo"]
        }
        fn run(
            &mut self,
            events: mpsc::Sender<ProviderEvent>,
            mut requests: mpsc::Receiver<ProviderRequest>,
            cancel: CancellationToken,
        ) -> Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + '_>> {
            Box::pin(async move {
                let mut counter = 0u64;
                loop {
                    tokio::select! {
                        _ = cancel.cancelled() => break,
                        req = requests.recv() => {
                            match req {
                                Some(ProviderRequest::Snapshot { reply, .. }) => {
                                    let _ = reply.send(Some(json!(counter)));
                                }
                                Some(ProviderRequest::Call { params, reply, .. }) => {
                                    let _ = reply.send(Ok(params));
                                }
                                None => break,
                            }
                        }
                        _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                            counter += 1;
                            let _ = events.send(ProviderEvent {
                                topic: "test.value".into(),
                                data: json!(counter),
                            }).await;
                        }
                    }
                }
                Ok(())
            })
        }
    }

    struct TestFactory;

    impl ProviderFactory for TestFactory {
        fn name(&self) -> &'static str {
            "test"
        }
        fn topics(&self) -> &'static [&'static str] {
            &["test.value"]
        }
        fn methods(&self) -> &'static [&'static str] {
            &["test.echo"]
        }
        fn create(&self) -> Box<dyn Provider> {
            Box::new(TestProvider)
        }
    }

    fn test_broker() -> (mpsc::Sender<BrokerMsg>, mpsc::Receiver<Response>) {
        let (broker, broker_tx) = Broker::new(vec![Box::new(TestFactory)]);
        tokio::spawn(broker.run());
        let (client_tx, client_rx) = mpsc::channel(16);
        let broker_tx2 = broker_tx.clone();
        tokio::spawn(async move {
            broker_tx2
                .send(BrokerMsg::ClientConnected {
                    id: 1,
                    tx: client_tx,
                })
                .await
                .unwrap();
        });
        (broker_tx, client_rx)
    }

    #[tokio::test]
    async fn subscribe_and_receive_events() {
        let (tx, mut rx) = test_broker();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Subscribe {
                pattern: "test.**".into(),
            },
        })
        .await
        .unwrap();

        let resp = rx.recv().await.unwrap();
        assert!(matches!(
            resp,
            Response::SubscribeAck {
                available: true,
                ..
            }
        ));

        let resp = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out")
            .unwrap();
        assert!(matches!(resp, Response::Event { .. }));
    }

    #[tokio::test]
    async fn disconnect_stops_provider() {
        let (tx, mut rx) = test_broker();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Subscribe {
                pattern: "test.**".into(),
            },
        })
        .await
        .unwrap();

        let _ = rx.recv().await; // ack
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        tx.send(BrokerMsg::ClientDisconnected { id: 1 })
            .await
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn subscribe_unknown_provider() {
        let (tx, mut rx) = test_broker();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Subscribe {
                pattern: "nonexistent.**".into(),
            },
        })
        .await
        .unwrap();

        let resp = rx.recv().await.unwrap();
        assert!(matches!(
            resp,
            Response::SubscribeAck {
                available: false,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn unsubscribe() {
        let (tx, mut rx) = test_broker();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Subscribe {
                pattern: "test.**".into(),
            },
        })
        .await
        .unwrap();

        // Drain ack + snapshot.
        let _ = rx.recv().await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        while rx.try_recv().is_ok() {}

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Unsubscribe {
                pattern: "test.**".into(),
            },
        })
        .await
        .unwrap();

        loop {
            let resp = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
                .await
                .expect("timed out")
                .unwrap();
            if matches!(resp, Response::UnsubscribeAck { .. }) {
                break;
            }
        }
    }

    #[tokio::test]
    async fn get_snapshot() {
        let (tx, mut rx) = test_broker();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Subscribe first to start the provider.
        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Subscribe {
                pattern: "test.**".into(),
            },
        })
        .await
        .unwrap();
        let _ = rx.recv().await; // ack
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Get {
                topic: "test.value".into(),
            },
        })
        .await
        .unwrap();

        loop {
            let resp = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
                .await
                .expect("timed out")
                .unwrap();
            if matches!(
                resp,
                Response::GetResult {
                    result: RequestResult::Ok { .. },
                    ..
                }
            ) {
                break;
            }
        }
    }

    #[tokio::test]
    async fn call_method() {
        let (tx, mut rx) = test_broker();
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Subscribe to start provider.
        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Subscribe {
                pattern: "test.**".into(),
            },
        })
        .await
        .unwrap();
        let _ = rx.recv().await; // ack
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        tx.send(BrokerMsg::Request {
            client: 1,
            request: Request::Call {
                method: "test.echo".into(),
                params: json!({"hello": "world"}),
            },
        })
        .await
        .unwrap();

        loop {
            let resp = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
                .await
                .expect("timed out")
                .unwrap();
            if let Response::CallResult { result, .. } = &resp {
                if let RequestResult::Ok { data } = result {
                    assert_eq!(*data, json!({"hello": "world"}));
                }
                break;
            }
        }
    }
}
