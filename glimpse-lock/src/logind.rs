use anyhow::Context;
use futures_util::StreamExt;
use glimpse_core::dbus::login1::{
    Login1ManagerProxy, Login1SessionEntry, Login1SessionProxy, SessionCandidate, current_uid,
    select_session_candidate,
};
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

    if let Ok(session_id) = std::env::var("XDG_SESSION_ID").map(|value| value.trim().to_owned()) {
        if session_id.is_empty() {
            tracing::debug!("ignoring empty XDG_SESSION_ID");
        } else {
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
    }

    match manager.get_session_by_pid(std::process::id()).await {
        Ok(path) => {
            tracing::debug!(session = %path, "resolved logind session from current process pid");
            Ok(path)
        }
        Err(error) => {
            tracing::debug!(%error, "failed to resolve logind session from current process pid");
            current_user_session_path(system, &manager)
                .await
                .context("resolve current logind session")
        }
    }
}

async fn current_user_session_path(
    system: &zbus::Connection,
    manager: &Login1ManagerProxy<'_>,
) -> anyhow::Result<OwnedObjectPath> {
    let uid = current_uid()?;
    let sessions = manager
        .list_sessions()
        .await
        .context("list logind sessions")?;
    let mut candidates = Vec::new();

    for entry in sessions {
        if entry.1 != uid {
            continue;
        }
        candidates.push(inspect_session_candidate(system, entry).await);
    }

    let selected = select_session_candidate(&candidates, uid)
        .ok_or_else(|| anyhow::anyhow!("no logind session for uid {uid}"))?;

    tracing::info!(
        uid,
        session_id = %selected.id,
        session = %selected.path,
        active = selected.active,
        class = ?selected.class,
        kind = ?selected.kind,
        seat = %selected.seat,
        "resolved logind session from user session list"
    );
    Ok(selected.path.clone())
}

async fn inspect_session_candidate(
    system: &zbus::Connection,
    entry: Login1SessionEntry,
) -> SessionCandidate {
    let (id, uid, _user, seat, path) = entry;
    let mut candidate = SessionCandidate {
        id,
        uid,
        seat,
        path,
        active: false,
        class: None,
        kind: None,
    };

    let proxy = match Login1SessionProxy::builder(system).path(candidate.path.clone()) {
        Ok(builder) => builder.build().await,
        Err(error) => Err(error),
    };

    match proxy {
        Ok(session) => {
            candidate.active = session.active().await.unwrap_or(false);
            candidate.class = session.class().await.ok();
            candidate.kind = session.kind().await.ok();
        }
        Err(error) => {
            tracing::debug!(
                %error,
                session_id = %candidate.id,
                session = %candidate.path,
                "failed to inspect logind session candidate"
            );
        }
    }

    candidate
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
