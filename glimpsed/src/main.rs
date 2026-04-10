mod broker;
mod pattern;
mod provider;
mod providers;
mod server;

use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{SignalKind, signal};
use tokio_util::sync::CancellationToken;

use crate::broker::Broker;

/// Try to bind the socket. If it already exists, check whether another instance
/// is running by attempting to connect. If the connection succeeds, bail. If it
/// fails (stale socket from a crash), remove and retry.
async fn bind_socket(path: &std::path::Path) -> anyhow::Result<UnixListener> {
    match UnixListener::bind(path) {
        Ok(listener) => return Ok(listener),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {}
        Err(e) => return Err(e.into()),
    }

    if UnixStream::connect(path).await.is_ok() {
        anyhow::bail!(
            "another glimpsed instance is already running on {}",
            path.display()
        );
    }

    tracing::info!("removing stale socket {}", path.display());
    std::fs::remove_file(path)?;
    Ok(UnixListener::bind(path)?)
}

fn register_providers() -> Vec<Box<dyn provider::ProviderFactory>> {
    vec![
        Box::new(providers::audio::AudioProviderFactory),
        Box::new(providers::battery::BatteryProviderFactory),
        Box::new(providers::brightness::BrightnessProviderFactory),
        Box::new(providers::calendar::CalendarProviderFactory),
        Box::new(providers::debug::DebugProviderFactory),
        Box::new(providers::network::NetworkProviderFactory),
        Box::new(providers::power::PowerProviderFactory),
        Box::new(providers::privacy::PrivacyProviderFactory),
    ]
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,system_tray=warn")),
        )
        .init();

    let path = glimpse_types::socket_path()?;
    let listener = bind_socket(&path).await?;
    tracing::info!("listening on {}", path.display());

    let cancel = CancellationToken::new();

    let (broker, broker_tx) = Broker::new(register_providers());
    tokio::spawn(broker.run());

    // Shutdown on SIGTERM / SIGINT.
    let shutdown = cancel.clone();
    tokio::spawn(async move {
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
        }
        tracing::info!("shutting down");
        shutdown.cancel();
    });

    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            result = listener.accept() => {
                let (stream, _addr) = result?;
                tracing::debug!("client connected");
                tokio::spawn(server::handle_client(stream, broker_tx.clone()));
            }
        }
    }

    // Drop broker sender so broker loop exits after all client tasks finish.
    drop(broker_tx);

    let _ = std::fs::remove_file(&path);
    tracing::info!("stopped");
    Ok(())
}
