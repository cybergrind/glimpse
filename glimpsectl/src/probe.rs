use glimpse::providers::battery::{BatteryEvent, BatteryProvider};
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
struct Output<'a> {
    event: &'a str,
    ts: i64,
    data: Value,
}
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::format::format_json;

pub type ProbeEvent = (String, Value);

pub async fn run(provider: &str, color: bool, pretty: bool) -> anyhow::Result<()> {
    let cancel = CancellationToken::new();
    let (tx, mut rx) = mpsc::channel::<ProbeEvent>(64);

    let cancel_clone = cancel.clone();
    let provider = provider.to_owned();
    tokio::spawn(async move {
        if let Err(e) = run_provider(&provider, tx, cancel_clone).await {
            eprintln!("error: {e}");
        }
    });

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        cancel.cancel();
    });

    while let Some((event, data)) = rx.recv().await {
        let ts = chrono::Utc::now().timestamp_millis();
        let value = serde_json::to_value(Output { event: &event, ts, data }).unwrap_or_default();
        println!("{}", format_json(&value, color, pretty));
    }

    Ok(())
}

async fn run_provider(
    name: &str,
    tx: mpsc::Sender<ProbeEvent>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    match name {
        "battery" => run_battery(tx, cancel).await,
        other => anyhow::bail!("unknown provider: {other}"),
    }
}

async fn run_battery(
    tx: mpsc::Sender<ProbeEvent>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    let mut provider = BatteryProvider::new(conn);
    let (event_tx, mut event_rx) = mpsc::channel::<BatteryEvent>(64);

    let bridge = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let (label, data) = match event {
                BatteryEvent::StatusChanged(s) => {
                    ("StatusChanged", serde_json::to_value(s).unwrap_or_default())
                }
                BatteryEvent::DevicesChanged(d) => {
                    ("DevicesChanged", serde_json::to_value(d).unwrap_or_default())
                }
            };
            if tx.send((label.to_string(), data)).await.is_err() {
                break;
            }
        }
    });

    provider.run(event_tx, cancel).await?;
    bridge.await.ok();
    Ok(())
}
