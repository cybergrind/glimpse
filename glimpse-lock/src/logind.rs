use anyhow::Context;
use futures_util::StreamExt;
use glimpse_core::dbus::login1::{Login1ManagerProxy, Login1SessionProxy};
use tokio::sync::mpsc;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogindLockEvent {
    Lock,
    Unlock,
}

pub async fn watch_lock_signals(sender: mpsc::Sender<LogindLockEvent>) -> anyhow::Result<()> {
    let system = zbus::Connection::system()
        .await
        .context("connect to system D-Bus")?;
    let session_path = current_session_path(&system).await?;
    let session = Login1SessionProxy::builder(&system)
        .path(session_path.clone())?
        .build()
        .await
        .with_context(|| format!("create logind session proxy for {session_path}"))?;
    let mut lock_signals = session.receive_lock().await?;
    let mut unlock_signals = session.receive_unlock().await?;

    tracing::info!(session = %session_path, "listening for logind lock requests");

    loop {
        tokio::select! {
            signal = lock_signals.next() => {
                if signal.is_none() {
                    anyhow::bail!("logind Lock signal stream ended");
                }
                if sender.send(LogindLockEvent::Lock).await.is_err() {
                    anyhow::bail!("lock app stopped receiving logind events");
                }
            }
            signal = unlock_signals.next() => {
                if signal.is_none() {
                    anyhow::bail!("logind Unlock signal stream ended");
                }
                if sender.send(LogindLockEvent::Unlock).await.is_err() {
                    anyhow::bail!("lock app stopped receiving logind events");
                }
            }
        }
    }
}

async fn current_session_path(system: &zbus::Connection) -> anyhow::Result<OwnedObjectPath> {
    let manager = Login1ManagerProxy::new(system)
        .await
        .context("create logind manager proxy")?;

    if let Ok(session_id) = std::env::var("XDG_SESSION_ID") {
        match manager.get_session(&session_id).await {
            Ok(path) => {
                tracing::debug!(session_id, session = %path, "resolved logind session from XDG_SESSION_ID");
                return Ok(path);
            }
            Err(error) => {
                tracing::debug!(%error, session_id, "failed to resolve logind session from XDG_SESSION_ID");
            }
        }
    }

    manager
        .get_session_by_pid(std::process::id())
        .await
        .context("resolve current logind session")
}

pub async fn set_current_session_locked_hint(locked: bool) -> anyhow::Result<()> {
    let system = zbus::Connection::system()
        .await
        .context("connect to system D-Bus")?;
    let session_path = current_session_path(&system).await?;
    set_session_locked_hint(&system, &session_path.as_ref(), locked).await
}

async fn set_session_locked_hint(
    system: &zbus::Connection,
    session_path: &ObjectPath<'_>,
    locked: bool,
) -> anyhow::Result<()> {
    let session = Login1SessionProxy::builder(system)
        .path(session_path)?
        .build()
        .await
        .with_context(|| format!("create logind session proxy for {session_path}"))?;
    session
        .set_locked_hint(locked)
        .await
        .with_context(|| format!("set logind LockedHint={locked}"))
}

#[cfg(test)]
mod tests {
    use super::LogindLockEvent;

    #[test]
    fn lock_events_are_distinct() {
        assert_ne!(LogindLockEvent::Lock, LogindLockEvent::Unlock);
    }
}
