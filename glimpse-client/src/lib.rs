use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use glimpse_types::{Request, RequestBody, RequestResult, Response, ResponseBody};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, Mutex};

/// Async client for the glimpsed daemon.
pub struct Client {
    writer: Arc<Mutex<tokio::net::unix::OwnedWriteHalf>>,
    next_id: AtomicU64,
    /// Pending one-shot requests: request_id → reply sender
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>>,
    /// Active subscriptions: request_id → event sender
    subscriptions: Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Response>>>>,
    _reader: tokio::task::JoinHandle<()>,
}

/// A stream of events from a subscription.
pub struct Subscription {
    rx: mpsc::UnboundedReceiver<Response>,
}

impl Subscription {
    /// Receive the next event. Returns `None` when the subscription ends.
    pub async fn next(&mut self) -> Option<(String, serde_json::Value)> {
        loop {
            let resp = self.rx.recv().await?;
            match resp.body {
                ResponseBody::Event { topic, data } => return Some((topic, data)),
                ResponseBody::ProviderUnavailable { provider, error } => {
                    tracing::warn!("provider {provider} unavailable: {error}");
                    return None;
                }
                ResponseBody::SubscribeAck { .. } => continue,
                _ => continue,
            }
        }
    }
}

impl Client {
    /// Connect to the daemon at the default socket path.
    pub async fn connect() -> anyhow::Result<Self> {
        let path = glimpse_types::socket_path()?;
        Self::connect_to(&path).await
    }

    /// Connect to a specific socket path.
    pub async fn connect_to(path: &std::path::Path) -> anyhow::Result<Self> {
        let stream = UnixStream::connect(path).await?;
        let (read_half, write_half) = stream.into_split();
        let writer = Arc::new(Mutex::new(write_half));
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let subscriptions: Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<Response>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = pending.clone();
        let subs_clone = subscriptions.clone();

        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(read_half).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let resp: Response = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("invalid response from daemon: {e}");
                        continue;
                    }
                };
                dispatch_response(resp, &pending_clone, &subs_clone).await;
            }
            pending_clone.lock().await.clear();
            subs_clone.lock().await.clear();
        });

        Ok(Self {
            writer,
            next_id: AtomicU64::new(1),
            pending,
            subscriptions,
            _reader: reader,
        })
    }

    /// One-shot read of a topic's current state.
    pub async fn get(&self, topic: &str) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(RequestBody::Get {
                topic: topic.into(),
            })
            .await?;
        match resp.body {
            ResponseBody::GetResult { result, .. } => match result {
                RequestResult::Ok { data } => Ok(data),
                RequestResult::Error { code, message } => {
                    anyhow::bail!("error {code}: {message}")
                }
            },
            _ => anyhow::bail!("unexpected response"),
        }
    }

    /// Subscribe to a topic pattern. Returns a `Subscription` that yields events.
    pub async fn subscribe(&self, pattern: &str) -> anyhow::Result<Subscription> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscriptions.lock().await.insert(id, tx);

        self.send(Request {
            id,
            body: RequestBody::Subscribe {
                pattern: pattern.into(),
            },
        })
        .await?;

        Ok(Subscription { rx })
    }

    /// Call a provider method.
    pub async fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let resp = self
            .request(RequestBody::Call {
                method: method.into(),
                params,
            })
            .await?;
        match resp.body {
            ResponseBody::CallResult { result, .. } => match result {
                RequestResult::Ok { data } => Ok(data),
                RequestResult::Error { code, message } => {
                    anyhow::bail!("error {code}: {message}")
                }
            },
            _ => anyhow::bail!("unexpected response"),
        }
    }

    async fn send(&self, request: Request) -> anyhow::Result<()> {
        let mut line = serde_json::to_string(&request)?;
        line.push('\n');
        let mut writer = self.writer.lock().await;
        writer.write_all(line.as_bytes()).await?;
        Ok(())
    }

    /// Send a request and wait for the matching response by id.
    async fn request(&self, body: RequestBody) -> anyhow::Result<Response> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.send(Request { id, body }).await?;
        rx.await.map_err(|_| anyhow::anyhow!("connection closed"))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self._reader.abort();
    }
}

async fn dispatch_response(
    resp: Response,
    pending: &Mutex<HashMap<u64, oneshot::Sender<Response>>>,
    subscriptions: &Mutex<HashMap<u64, mpsc::UnboundedSender<Response>>>,
) {
    let id = resp.id;

    // Check pending one-shot requests first (Get, Call, Unsubscribe).
    {
        let mut pending = pending.lock().await;
        if let Some(tx) = pending.remove(&id) {
            let _ = tx.send(resp);
            return;
        }
    }

    // Check subscriptions (SubscribeAck, Event, ProviderUnavailable).
    {
        let mut subs = subscriptions.lock().await;
        if let Some(tx) = subs.get(&id) {
            if tx.send(resp).is_err() {
                subs.remove(&id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    static TEST_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    async fn mock_daemon() -> std::path::PathBuf {
        let n = TEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("glimpse-test-{}-{n}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();

        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let (read, write) = stream.into_split();
                    let mut lines = BufReader::new(read).lines();
                    let mut write = write;

                    while let Ok(Some(line)) = lines.next_line().await {
                        let req: Request = serde_json::from_str(&line).unwrap();
                        let body = match req.body {
                            RequestBody::Get { topic } => ResponseBody::GetResult {
                                topic,
                                result: RequestResult::Ok {
                                    data: serde_json::json!(42),
                                },
                            },
                            RequestBody::Subscribe { pattern } => ResponseBody::SubscribeAck {
                                pattern,
                                available: true,
                                error: None,
                            },
                            RequestBody::Call { method, params } => ResponseBody::CallResult {
                                method,
                                result: RequestResult::Ok { data: params },
                            },
                            RequestBody::Unsubscribe { pattern } => {
                                ResponseBody::UnsubscribeAck { pattern }
                            }
                        };
                        let resp = Response { id: req.id, body };
                        let mut json = serde_json::to_string(&resp).unwrap();
                        json.push('\n');
                        if write.write_all(json.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        path
    }

    #[tokio::test]
    async fn get_returns_data() {
        let path = mock_daemon().await;
        let client = Client::connect_to(&path).await.unwrap();
        let data = client.get("test.topic").await.unwrap();
        assert_eq!(data, serde_json::json!(42));
    }

    #[tokio::test]
    async fn call_returns_params() {
        let path = mock_daemon().await;
        let client = Client::connect_to(&path).await.unwrap();
        let data = client
            .call("test.method", serde_json::json!({"x": 1}))
            .await
            .unwrap();
        assert_eq!(data, serde_json::json!({"x": 1}));
    }

    #[tokio::test]
    async fn subscribe_receives_ack() {
        let path = mock_daemon().await;
        let client = Client::connect_to(&path).await.unwrap();
        let _sub = client.subscribe("test.**").await.unwrap();
    }

    #[tokio::test]
    async fn concurrent_gets() {
        let path = mock_daemon().await;
        let client = Arc::new(Client::connect_to(&path).await.unwrap());
        let c1 = client.clone();
        let c2 = client.clone();

        let (r1, r2) = tokio::join!(
            tokio::spawn(async move { c1.get("topic.a").await.unwrap() }),
            tokio::spawn(async move { c2.get("topic.b").await.unwrap() }),
        );

        assert_eq!(r1.unwrap(), serde_json::json!(42));
        assert_eq!(r2.unwrap(), serde_json::json!(42));
    }
}
