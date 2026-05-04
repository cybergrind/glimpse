use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    dbus::power_profiles::PowerProfilesDaemonProxy,
    services::framework::{Control, ServiceCommand, ServiceHandle},
};

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct PowerProfiles {
    pub active: String,
    pub available: Vec<String>,
    pub performance_degraded: String,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct State {
    pub profiles: PowerProfiles,
}

#[derive(Debug, Clone)]
pub enum Command {
    Refresh,
    SetProfile(String),
}

pub type PowerHandle = ServiceHandle<State, Command>;

pub struct PowerService {
    conn: zbus::Connection,
    state_tx: watch::Sender<State>,
    command_rx: mpsc::Receiver<ServiceCommand<Command>>,
}

impl PowerService {
    pub fn new(conn: zbus::Connection) -> (Self, PowerHandle) {
        let (state_tx, state_rx) = watch::channel(State::default());
        let (command_tx, command_rx) = mpsc::channel(8);

        (
            Self {
                conn,
                state_tx,
                command_rx,
            },
            ServiceHandle::new(state_rx, command_tx),
        )
    }

    fn change_state(&self, state: State) {
        if let Err(err) = self.state_tx.send(state) {
            tracing::error!("failed to send new power state: {:?}", err);
        }
    }

    pub async fn run(mut self, cancel: CancellationToken) {
        if let Err(error) = self.run_inner(cancel).await {
            tracing::warn!(error = %error, "power service failed");
        }
    }

    async fn run_inner(&mut self, cancel: CancellationToken) -> anyhow::Result<()> {
        let conn = self.conn.clone();
        let mut state = self.state_tx.borrow().clone();
        let mut pp = connect_profiles(&conn, &mut state).await;

        self.change_state(state);

        let mut profile_changes = match &pp {
            Some(pp) => Some(pp.receive_active_profile_changed().await),
            None => None,
        };

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(_) = async {
                    match &mut profile_changes {
                        Some(stream) => stream.next().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if let Some(ref pp) = pp {
                        let mut next = self.state_tx.borrow().clone();
                        if read_profiles(pp, &mut next).await {
                            self.change_state(next);
                        }
                    }
                }
                command = self.command_rx.recv() => match command {
                    Some(ServiceCommand::Command(command)) => {
                        let refresh = self.execute_command(command).await;
                        if refresh {
                            let mut next = self.state_tx.borrow().clone();
                            if pp.is_none() {
                                pp = connect_profiles(&conn, &mut next).await;
                                profile_changes = match &pp {
                                    Some(pp) => Some(pp.receive_active_profile_changed().await),
                                    None => None,
                                };
                            } else if let Some(ref pp) = pp {
                                read_profiles(pp, &mut next).await;
                            }
                            if should_emit_state(&self.state_tx.borrow(), &next) {
                                self.change_state(next);
                            }
                        }
                    }
                    Some(ServiceCommand::Control(control)) => match control {
                        Control::Start(_) | Control::Reconfigure(_) => {
                            let mut next = self.state_tx.borrow().clone();
                            if pp.is_none() {
                                pp = connect_profiles(&conn, &mut next).await;
                                profile_changes = match &pp {
                                    Some(pp) => Some(pp.receive_active_profile_changed().await),
                                    None => None,
                                };
                            } else if let Some(ref pp) = pp {
                                read_profiles(pp, &mut next).await;
                            }
                            if should_emit_state(&self.state_tx.borrow(), &next) {
                                self.change_state(next);
                            }
                        }
                        Control::Shutdown => break,
                    },
                    None => break,
                }
            }
        }

        Ok(())
    }

    async fn execute_command(&self, command: Command) -> bool {
        match command {
            Command::Refresh => true,
            Command::SetProfile(profile) => {
                if let Err(error) = self.set_profile(&profile).await {
                    tracing::warn!(error = %error, "power command failed");
                }
                true
            }
        }
    }

    async fn set_profile(&self, profile: &str) -> anyhow::Result<()> {
        let pp = PowerProfilesDaemonProxy::new(&self.conn).await?;
        pp.set_active_profile(profile)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}

async fn connect_profiles<'a>(
    conn: &'a zbus::Connection,
    state: &mut State,
) -> Option<PowerProfilesDaemonProxy<'a>> {
    match PowerProfilesDaemonProxy::new(conn).await {
        Ok(pp) => {
            read_profiles(&pp, state).await;
            tracing::info!(
                active = %state.profiles.active,
                available = ?state.profiles.available,
                "power: profiles loaded"
            );
            Some(pp)
        }
        Err(error) => {
            tracing::warn!(error = %error, "power: power-profiles-daemon not available");
            None
        }
    }
}

async fn read_profiles(pp: &PowerProfilesDaemonProxy<'_>, state: &mut State) -> bool {
    let raw = pp.profiles().await.unwrap_or_default();
    let next = PowerProfiles {
        active: pp.active_profile().await.unwrap_or_default(),
        performance_degraded: pp.performance_degraded().await.unwrap_or_default(),
        available: raw
            .iter()
            .filter_map(|entry| {
                entry.get("Profile").and_then(|value| {
                    use zbus::zvariant::Value;
                    match &**value {
                        Value::Str(profile) => Some(profile.to_string()),
                        _ => None,
                    }
                })
            })
            .collect(),
    };

    let changed = state.profiles != next;
    state.profiles = next;
    changed
}

fn should_emit_state(previous: &State, next: &State) -> bool {
    previous != next
}

#[cfg(test)]
mod tests {
    use super::{PowerProfiles, State};

    #[test]
    fn should_emit_state_only_for_real_changes() {
        let previous = State {
            profiles: PowerProfiles {
                active: "balanced".into(),
                available: vec![
                    "power-saver".into(),
                    "balanced".into(),
                    "performance".into(),
                ],
                performance_degraded: String::new(),
            },
            ..State::default()
        };

        assert!(!super::should_emit_state(&previous, &previous));

        let mut next = previous.clone();
        next.profiles.active = "performance".into();
        assert!(super::should_emit_state(&previous, &next));
    }
}
