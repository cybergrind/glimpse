use serde::Serialize;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use zbus::zvariant::OwnedObjectPath;

use crate::{
    dbus::login1::Login1ManagerProxy,
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionActionAvailability {
    Available,
    Challenge,
    #[default]
    Unavailable,
    Unknown(String),
}

impl SessionActionAvailability {
    pub fn from_login1(value: &str) -> Self {
        match value {
            "yes" => Self::Available,
            "challenge" => Self::Challenge,
            "no" | "na" => Self::Unavailable,
            other => Self::Unknown(other.to_owned()),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionBackendState {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionActionCapabilities {
    pub backend: SessionBackendState,
    pub suspend: SessionActionAvailability,
    pub hibernate: SessionActionAvailability,
    pub reboot: SessionActionAvailability,
    pub power_off: SessionActionAvailability,
    pub lock: SessionActionAvailability,
}

impl Default for SessionActionCapabilities {
    fn default() -> Self {
        Self::unavailable()
    }
}

impl SessionActionCapabilities {
    pub fn unavailable() -> Self {
        Self {
            backend: SessionBackendState::Unavailable,
            suspend: SessionActionAvailability::Unavailable,
            hibernate: SessionActionAvailability::Unavailable,
            reboot: SessionActionAvailability::Unavailable,
            power_off: SessionActionAvailability::Unavailable,
            lock: SessionActionAvailability::Unavailable,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionServiceHealth {
    Ready,
    Degraded { message: String },
}

impl Default for SessionServiceHealth {
    fn default() -> Self {
        Self::Degraded {
            message: "Session actions unavailable".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub capabilities: SessionActionCapabilities,
    pub user_name: String,
    pub host_name: String,
    pub uptime: String,
    pub subtitle: String,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            capabilities: SessionActionCapabilities::default(),
            user_name: "user".into(),
            host_name: "linux".into(),
            uptime: String::new(),
            subtitle: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SessionAction {
    Lock,
    Logout,
    Suspend,
    Hibernate,
    Reboot,
    PowerOff,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct State {
    pub health: SessionServiceHealth,
    pub snapshot: SessionSnapshot,
    pub active_action: Option<SessionAction>,
}

#[derive(Debug, Clone)]
pub enum Command {
    Refresh,
    Run(SessionAction),
}

pub type SessionHandle = ServiceHandle<State, Command>;

pub struct SessionService {
    conn: zbus::Connection,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
    action_tx: mpsc::Sender<ActionResult>,
    action_rx: mpsc::Receiver<ActionResult>,
}

#[derive(Debug)]
struct ActionResult {
    action: SessionAction,
    result: anyhow::Result<()>,
}

impl SessionService {
    pub fn new(conn: zbus::Connection) -> (Self, SessionHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(8);
        let (action_tx, action_rx) = mpsc::channel(8);

        (
            Self {
                conn,
                state_tx,
                command_rx,
                action_tx,
                action_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        if let Err(error) = self.run_inner(cancel).await {
            tracing::warn!(error = %error, "session service failed");
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<()> {
        self.refresh().await;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(action_result) = self.action_rx.recv() => {
                    self.finish_action(action_result).await;
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(Command::Refresh)) => {
                        self.refresh().await;
                    }
                    Some(ServiceCommand::Command(Command::Run(action))) => {
                        self.execute_action(action).await;
                    }
                    Some(ServiceCommand::Control(control)) => match control {
                        Control::Start(_) | Control::Reconfigure(_) => {
                            self.refresh().await;
                        }
                        Control::Shutdown => break,
                    },
                    None => break,
                }
            }
        }

        Ok(())
    }

    async fn refresh(&self) {
        let mut next = self.state_tx.borrow().clone();
        let (health, snapshot) = self.read_snapshot().await;
        next.health = health;
        next.snapshot = snapshot;
        if should_emit_state(&self.state_tx.borrow(), &next) {
            self.change_state(next);
        }
    }

    async fn execute_action(&self, action: SessionAction) {
        if self.state_tx.borrow().active_action.is_some() {
            tracing::debug!(
                ?action,
                "session action ignored while another action is active"
            );
            return;
        }

        self.set_active_action(Some(action));

        let worker = ActionWorker {
            conn: self.conn.clone(),
        };
        let action_tx = self.action_tx.clone();
        tokio::spawn(async move {
            let result = worker.run(action).await;
            if action_tx
                .send(ActionResult { action, result })
                .await
                .is_err()
            {
                tracing::debug!(?action, "session action result dropped");
            }
        });
    }

    async fn finish_action(&self, action_result: ActionResult) {
        if let Err(error) = action_result.result {
            tracing::warn!(action = ?action_result.action, error = %error, "session command failed");
        }
        self.set_active_action(None);
        self.refresh().await;
    }

    fn set_active_action(&self, active_action: Option<SessionAction>) {
        let mut next = self.state_tx.borrow().clone();
        next.active_action = active_action;
        if should_emit_state(&self.state_tx.borrow(), &next) {
            self.change_state(next);
        }
    }

    fn change_state(&self, state: State) {
        if let Err(error) = self.state_tx.send(state) {
            tracing::error!(?error, "failed to send new session state");
        }
    }

    async fn read_snapshot(&self) -> (SessionServiceHealth, SessionSnapshot) {
        let host_name = read_hostname();
        let uptime = read_uptime().map(format_uptime).unwrap_or_default();
        let (health, capabilities) = self.capabilities().await;

        (
            health,
            SessionSnapshot {
                capabilities,
                user_name: fallback_user_name(),
                uptime: uptime.clone(),
                subtitle: build_subtitle(&host_name, &uptime),
                host_name,
            },
        )
    }

    async fn capabilities(&self) -> (SessionServiceHealth, SessionActionCapabilities) {
        let manager = match self.logind().await {
            Ok(manager) => manager,
            Err(error) => {
                tracing::warn!(error = %error, "session actions unavailable");
                return (
                    SessionServiceHealth::Degraded {
                        message: "Session actions unavailable".into(),
                    },
                    SessionActionCapabilities::unavailable(),
                );
            }
        };

        let mut failures = Vec::new();
        let capabilities = SessionActionCapabilities {
            backend: SessionBackendState::Available,
            suspend: capability_from_result("suspend", manager.can_suspend().await, &mut failures),
            hibernate: capability_from_result(
                "hibernate",
                manager.can_hibernate().await,
                &mut failures,
            ),
            reboot: capability_from_result("reboot", manager.can_reboot().await, &mut failures),
            power_off: capability_from_result(
                "power-off",
                manager.can_power_off().await,
                &mut failures,
            ),
            lock: SessionActionAvailability::Available,
        };

        let health = if failures.is_empty() {
            SessionServiceHealth::Ready
        } else {
            tracing::debug!(?failures, "session capability queries failed");
            SessionServiceHealth::Degraded {
                message: "Some session actions are unavailable".into(),
            }
        };

        (health, capabilities)
    }

    async fn logind(&self) -> anyhow::Result<Login1ManagerProxy<'_>> {
        Login1ManagerProxy::new(&self.conn)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }
}

struct ActionWorker {
    conn: zbus::Connection,
}

impl ActionWorker {
    async fn run(&self, action: SessionAction) -> anyhow::Result<()> {
        match action {
            SessionAction::Lock => self.lock().await,
            SessionAction::Logout => self.logout().await,
            SessionAction::Suspend => self.suspend().await,
            SessionAction::Hibernate => self.hibernate().await,
            SessionAction::Reboot => self.reboot().await,
            SessionAction::PowerOff => self.power_off().await,
        }
    }

    async fn lock(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .lock_sessions()
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    async fn logout(&self) -> anyhow::Result<()> {
        let session_id = self
            .resolve_logout_session_id()
            .await?
            .ok_or_else(|| anyhow::anyhow!("could not resolve current logind session"))?;

        self.logind()
            .await?
            .terminate_session(&session_id)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    async fn resolve_logout_session_id(&self) -> anyhow::Result<Option<String>> {
        if let Some(session_id) = resolve_logout_session_id(|key| std::env::var(key)) {
            return Ok(Some(session_id));
        }

        let manager = self.logind().await?;
        let path = manager.get_session_by_pid(std::process::id()).await?;
        let sessions = manager.list_sessions().await?;
        Ok(session_id_for_path(&sessions, &path))
    }

    async fn suspend(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .suspend(true)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    async fn hibernate(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .hibernate(true)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    async fn reboot(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .reboot(true)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    async fn power_off(&self) -> anyhow::Result<()> {
        self.logind()
            .await?
            .power_off(true)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }

    async fn logind(&self) -> anyhow::Result<Login1ManagerProxy<'_>> {
        Login1ManagerProxy::new(&self.conn)
            .await
            .map_err(|error| anyhow::anyhow!("{error}"))
    }
}

fn capability_from_result(
    action: &'static str,
    result: zbus::Result<String>,
    failures: &mut Vec<&'static str>,
) -> SessionActionAvailability {
    match result {
        Ok(value) => SessionActionAvailability::from_login1(&value),
        Err(error) => {
            tracing::debug!(action, error = %error, "session capability query failed");
            failures.push(action);
            SessionActionAvailability::Unavailable
        }
    }
}

pub fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let mins = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

pub fn build_subtitle(host_name: &str, uptime: &str) -> String {
    if uptime.is_empty() {
        host_name.to_owned()
    } else {
        format!("{host_name}, up {uptime}")
    }
}

pub fn resolve_logout_session_id<F>(env: F) -> Option<String>
where
    F: for<'a> Fn(&'a str) -> Result<String, std::env::VarError>,
{
    env("XDG_SESSION_ID")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn read_hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .map(|value| value.trim().to_owned())
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "linux".into())
}

fn read_uptime() -> Option<u64> {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|value| value.split_whitespace().next()?.parse::<f64>().ok())
        .map(|seconds| seconds as u64)
}

fn session_id_for_path(
    sessions: &[crate::dbus::login1::Login1SessionEntry],
    path: &OwnedObjectPath,
) -> Option<String> {
    sessions
        .iter()
        .find(|(_, _, _, _, session_path)| session_path == path)
        .map(|(id, _, _, _, _)| id.clone())
}

fn fallback_user_name() -> String {
    std::env::var("USER")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "user".into())
}

fn should_emit_state(previous: &State, next: &State) -> bool {
    previous != next
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_action_availability_maps_login1_values() {
        assert_eq!(
            SessionActionAvailability::from_login1("yes"),
            SessionActionAvailability::Available
        );
        assert_eq!(
            SessionActionAvailability::from_login1("challenge"),
            SessionActionAvailability::Challenge
        );
        assert_eq!(
            SessionActionAvailability::from_login1("no"),
            SessionActionAvailability::Unavailable
        );
        assert_eq!(
            SessionActionAvailability::from_login1("na"),
            SessionActionAvailability::Unavailable
        );
    }

    #[test]
    fn session_action_availability_preserves_unknown_values() {
        assert_eq!(
            SessionActionAvailability::from_login1("limited"),
            SessionActionAvailability::Unknown("limited".into())
        );
    }

    #[test]
    fn format_uptime_displays_compact_duration() {
        assert_eq!(format_uptime(65), "1m");
        assert_eq!(format_uptime(3720), "1h 2m");
        assert_eq!(format_uptime(90061), "1d 1h");
    }

    #[test]
    fn build_subtitle_uses_hostname_and_optional_uptime() {
        assert_eq!(build_subtitle("workstation", ""), "workstation");
        assert_eq!(
            build_subtitle("workstation", "2h 5m"),
            "workstation, up 2h 5m"
        );
    }

    #[test]
    fn default_state_is_deterministic_and_degraded() {
        let state = State::default();

        assert_eq!(
            state.health,
            SessionServiceHealth::Degraded {
                message: "Session actions unavailable".into()
            }
        );
        assert_eq!(state.snapshot.user_name, "user");
        assert!(state.active_action.is_none());
    }

    #[test]
    fn resolve_logout_session_id_reads_xdg_session_id() {
        let env = [("XDG_SESSION_ID", "c2"), ("USER", "alex")];
        assert_eq!(
            resolve_logout_session_id(|key| {
                env.iter()
                    .find(|(name, _)| *name == key)
                    .map(|(_, value)| (*value).to_string())
                    .ok_or(std::env::VarError::NotPresent)
            })
            .as_deref(),
            Some("c2")
        );
    }

    #[test]
    fn session_id_for_path_matches_logind_session_entry() {
        let path = OwnedObjectPath::try_from("/org/freedesktop/login1/session/_32").unwrap();
        let sessions = vec![
            (
                "1".into(),
                1000,
                "alex".into(),
                "seat0".into(),
                OwnedObjectPath::try_from("/org/freedesktop/login1/session/_31").unwrap(),
            ),
            (
                "2".into(),
                1000,
                "alex".into(),
                "seat0".into(),
                path.clone(),
            ),
        ];

        assert_eq!(session_id_for_path(&sessions, &path).as_deref(), Some("2"));
    }
}
