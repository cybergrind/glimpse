use zbus::zvariant::OwnedObjectPath;

pub type Login1SessionEntry = (String, u32, String, String, OwnedObjectPath);

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
pub trait Login1Manager {
    fn get_session(&self, session_id: &str) -> zbus::Result<OwnedObjectPath>;
    fn get_session_by_pid(&self, pid: u32) -> zbus::Result<OwnedObjectPath>;
    fn list_sessions(&self) -> zbus::Result<Vec<Login1SessionEntry>>;
    fn can_suspend(&self) -> zbus::Result<String>;
    fn can_hibernate(&self) -> zbus::Result<String>;
    fn can_reboot(&self) -> zbus::Result<String>;
    fn can_power_off(&self) -> zbus::Result<String>;
    fn suspend(&self, interactive: bool) -> zbus::Result<()>;
    fn hibernate(&self, interactive: bool) -> zbus::Result<()>;
    fn reboot(&self, interactive: bool) -> zbus::Result<()>;
    fn power_off(&self, interactive: bool) -> zbus::Result<()>;
    fn lock_session(&self, session_id: &str) -> zbus::Result<()>;
    fn lock_sessions(&self) -> zbus::Result<()>;
    fn terminate_session(&self, session_id: &str) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1"
)]
pub trait Login1Session {
    fn set_locked_hint(&self, locked: bool) -> zbus::Result<()>;
    fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;

    #[zbus(property)]
    fn active(&self) -> zbus::Result<bool>;
    #[zbus(property, name = "Class")]
    fn class(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn seat(&self) -> zbus::Result<(String, OwnedObjectPath)>;
    #[zbus(property, name = "Type")]
    fn kind(&self) -> zbus::Result<String>;

    #[zbus(signal)]
    fn lock(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn unlock(&self) -> zbus::Result<()>;
}

#[derive(Clone, Debug)]
pub struct SessionCandidate {
    pub id: String,
    pub uid: u32,
    pub seat: String,
    pub path: OwnedObjectPath,
    pub active: bool,
    pub class: Option<String>,
    pub kind: Option<String>,
}

pub fn select_session_candidate(
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
        select_best_session(candidates.iter().copied().filter(|candidate| {
            candidate.class.as_deref() == Some("user") && candidate.kind.as_deref() == Some("x11")
        }))
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
    if candidate.kind.as_deref() == Some("x11") {
        score += 20;
    }
    if !candidate.seat.is_empty() {
        score += 10;
    }
    score
}

pub fn current_uid() -> anyhow::Result<u32> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let uid = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|value| value.split_whitespace().next())
        .ok_or_else(|| anyhow::anyhow!("read uid from /proc/self/status"))?;
    uid.parse()
        .map_err(|error| anyhow::anyhow!("parse uid from /proc/self/status: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{SessionCandidate, select_session_candidate};
    use zbus::zvariant::OwnedObjectPath;

    #[test]
    fn select_session_candidate_prefers_active_user_wayland_session() {
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
    fn select_session_candidate_prefers_graphical_session_over_active_tty() {
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
    fn select_session_candidate_ignores_other_users() {
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
