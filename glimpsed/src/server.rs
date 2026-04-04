use std::sync::atomic::{AtomicU64, Ordering};

use glimpse_types::{Request, Response};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use crate::broker::BrokerMsg;

pub type ClientId = u64;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_client_id() -> ClientId {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub async fn handle_client(stream: UnixStream, broker_tx: mpsc::Sender<BrokerMsg>) {
    let id = next_client_id();
    let (read_half, write_half) = stream.into_split();
    let (response_tx, mut response_rx) = mpsc::channel::<Response>(64);

    if broker_tx
        .send(BrokerMsg::ClientConnected {
            id,
            tx: response_tx,
        })
        .await
        .is_err()
    {
        return;
    }

    let broker_reader = broker_tx.clone();

    let mut reader = tokio::spawn(async move {
        let mut lines = BufReader::new(read_half).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let request: Request = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(client = id, "invalid request: {e}");
                    continue;
                }
            };
            if broker_reader
                .send(BrokerMsg::Request {
                    client: id,
                    request,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let mut writer = tokio::spawn(async move {
        let mut write_half = write_half;
        while let Some(response) = response_rx.recv().await {
            let mut line = match serde_json::to_string(&response) {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(client = id, "failed to serialize response: {e}");
                    continue;
                }
            };
            line.push('\n');
            if write_half.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = &mut reader => { writer.abort(); }
        _ = &mut writer => { reader.abort(); }
    }

    let _ = broker_tx.send(BrokerMsg::ClientDisconnected { id }).await;
    tracing::debug!(client = id, "disconnected");
}
