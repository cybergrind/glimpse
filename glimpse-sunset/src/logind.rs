use anyhow::Context;
use futures_util::StreamExt;
use glimpse_core::dbus::login1::Login1ManagerProxy;
use tokio::sync::mpsc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepEvent {
    Suspending,
    Resumed,
}

pub async fn watch_sleep_events(sender: mpsc::Sender<SleepEvent>) -> anyhow::Result<()> {
    let system = zbus::Connection::system()
        .await
        .context("connect to system D-Bus")?;
    let manager = Login1ManagerProxy::new(&system)
        .await
        .context("create logind manager proxy")?;
    let mut signals = manager.receive_prepare_for_sleep().await?;

    tracing::info!("listening for logind sleep events");

    while let Some(signal) = signals.next().await {
        let args = signal
            .args()
            .context("read logind PrepareForSleep signal")?;
        let event = if args.start {
            SleepEvent::Suspending
        } else {
            SleepEvent::Resumed
        };

        if sender.send(event).await.is_err() {
            anyhow::bail!("sunset app stopped receiving logind sleep events");
        }
    }

    anyhow::bail!("logind PrepareForSleep signal stream ended")
}
