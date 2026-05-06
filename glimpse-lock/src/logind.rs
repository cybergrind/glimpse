use anyhow::Context;
use futures_util::StreamExt;
use glimpse_core::dbus::login1::{Login1ManagerProxy, Login1SessionEntry, Login1SessionProxy};
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

#[derive(Clone, Debug)]
struct SessionCandidate {
    id: String,
    uid: u32,
    seat: String,
    path: OwnedObjectPath,
    active: bool,
    class: Option<String>,
    kind: Option<String>,
}

fn select_session_candidate(
    candidates: &[SessionCandidate],
    uid: u32,
) -> Option<&SessionCandidate> {
    let candidates = candidates
        .iter()
        .filter(|candidate| candidate.uid == uid)
        .collect::<Vec<_>>();

    select_best_session(candidates.iter().copied().filter(|candidate| {
        candidate.class.as_deref() == Some("user") && candidate.kind.as_deref() == Some("wayland")
    }))
    .or_else(|| {
        select_best_session(
            candidates
                .iter()
                .copied()
                .filter(|candidate| candidate.kind.as_deref() == Some("wayland")),
        )
    })
    .or_else(|| {
        select_best_session(
            candidates
                .iter()
                .copied()
                .filter(|candidate| candidate.class.as_deref() == Some("user")),
        )
    })
    .or_else(|| select_best_session(candidates.iter().copied()))
}

fn select_best_session<'a>(
    candidates: impl Iterator<Item = &'a SessionCandidate>,
) -> Option<&'a SessionCandidate> {
    candidates.max_by_key(|candidate| session_candidate_score(candidate))
}

fn session_candidate_score(candidate: &SessionCandidate) -> u32 {
    let mut score = 0;
    if candidate.active {
        score += 100;
    }
    if candidate.class.as_deref() == Some("user") {
        score += 40;
    }
    if candidate.kind.as_deref() == Some("wayland") {
        score += 30;
    }
    if !candidate.seat.is_empty() {
        score += 10;
    }
    score
}

fn current_uid() -> anyhow::Result<u32> {
    let status = std::fs::read_to_string("/proc/self/status").context("read /proc/self/status")?;
    let uid = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|value| value.split_whitespace().next())
        .ok_or_else(|| anyhow::anyhow!("read uid from /proc/self/status"))?;
    uid.parse().context("parse uid from /proc/self/status")
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
    use super::{LogindLockEvent, SessionCandidate, select_session_candidate};
    use zbus::zvariant::OwnedObjectPath;

    #[test]
    fn lock_events_are_distinct() {
        assert_ne!(LogindLockEvent::Lock, LogindLockEvent::Unlock);
    }

    #[test]
    fn selects_active_user_wayland_session_for_current_uid() {
        let candidates = vec![
            candidate("1", 1000, false, Some("user"), Some("wayland"), "seat0"),
            candidate("2", 1000, true, Some("user"), Some("wayland"), "seat0"),
            candidate("3", 1001, true, Some("user"), Some("wayland"), "seat0"),
        ];

        assert_eq!(
            select_session_candidate(&candidates, 1000).map(|candidate| candidate.id.as_str()),
            Some("2")
        );
    }

    #[test]
    fn selects_local_user_session_when_active_property_is_unavailable() {
        let candidates = vec![
            candidate("1", 1000, false, Some("greeter"), Some("wayland"), "seat0"),
            candidate("2", 1000, false, Some("user"), Some("wayland"), "seat0"),
        ];

        assert_eq!(
            select_session_candidate(&candidates, 1000).map(|candidate| candidate.id.as_str()),
            Some("2")
        );
    }

    #[test]
    fn prefers_wayland_session_over_active_non_graphical_session() {
        let candidates = vec![
            candidate("1", 1000, true, Some("user"), Some("tty"), "seat0"),
            candidate("2", 1000, false, Some("user"), Some("wayland"), "seat0"),
        ];

        assert_eq!(
            select_session_candidate(&candidates, 1000).map(|candidate| candidate.id.as_str()),
            Some("2")
        );
    }

    #[test]
    fn ignores_sessions_for_other_users() {
        let candidates = vec![candidate(
            "1",
            1001,
            true,
            Some("user"),
            Some("wayland"),
            "seat0",
        )];

        assert!(select_session_candidate(&candidates, 1000).is_none());
    }

    fn candidate(
        id: &str,
        uid: u32,
        active: bool,
        class: Option<&str>,
        kind: Option<&str>,
        seat: &str,
    ) -> SessionCandidate {
        SessionCandidate {
            id: id.into(),
            uid,
            seat: seat.into(),
            path: OwnedObjectPath::try_from(format!("/org/freedesktop/login1/session/_{id}"))
                .unwrap(),
            active,
            class: class.map(str::to_owned),
            kind: kind.map(str::to_owned),
        }
    }
}
