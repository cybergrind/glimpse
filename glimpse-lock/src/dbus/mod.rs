use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use relm4::Sender;

use crate::app::AppCommand;

pub const LOCK_OBJECT_PATH: &str = "/me/aresa/GlimpseLock";

#[derive(Clone, Default)]
pub struct LockApiState {
    inner: Arc<LockApiStateInner>,
}

#[derive(Default)]
struct LockApiStateInner {
    active: AtomicBool,
    was_active: AtomicBool,
    active_since: Mutex<Option<Instant>>,
}

impl LockApiState {
    pub fn set_active(&self, active: bool) {
        self.inner.active.store(active, Ordering::Relaxed);
        if active {
            self.inner.was_active.store(true, Ordering::Relaxed);
        }
        let Ok(mut active_since) = self.inner.active_since.lock() else {
            tracing::warn!("lock API state mutex is poisoned");
            return;
        };
        *active_since = active.then(Instant::now);
    }

    pub fn was_ever_active(&self) -> bool {
        self.inner.was_active.load(Ordering::Relaxed)
    }

    fn active(&self) -> bool {
        self.inner.active.load(Ordering::Relaxed)
    }

    fn active_time(&self) -> u32 {
        let Ok(active_since) = self.inner.active_since.lock() else {
            tracing::warn!("lock API state mutex is poisoned");
            return 0;
        };
        active_since
            .map(|started| started.elapsed().as_secs().min(u32::MAX as u64) as u32)
            .unwrap_or(0)
    }
}

pub async fn register_lock_api(
    connection: zbus::Connection,
    sender: Sender<AppCommand>,
    state: LockApiState,
) -> zbus::Result<()> {
    connection
        .object_server()
        .at(LOCK_OBJECT_PATH, LockApi { sender, state })
        .await?;
    tracing::info!(path = LOCK_OBJECT_PATH, "glimpse-lock D-Bus API registered");
    Ok(())
}

struct LockApi {
    sender: Sender<AppCommand>,
    state: LockApiState,
}

#[zbus::interface(name = "me.aresa.GlimpseLock")]
impl LockApi {
    #[zbus(name = "Lock")]
    async fn lock(&self) -> zbus::fdo::Result<()> {
        self.sender
            .send(AppCommand::RequestLock)
            .map_err(|_| zbus::fdo::Error::Failed("lock app is not running".into()))
    }

    #[zbus(name = "GetActive")]
    fn get_active(&self) -> bool {
        self.state.active()
    }

    #[zbus(name = "GetActiveTime")]
    fn get_active_time(&self) -> u32 {
        self.state.active_time()
    }
}

#[cfg(test)]
mod tests {
    use super::LockApiState;

    #[test]
    fn active_time_is_zero_when_unlocked() {
        let state = LockApiState::default();

        assert_eq!(state.active_time(), 0);
        state.set_active(true);
        state.set_active(false);
        assert_eq!(state.active_time(), 0);
    }

    #[test]
    fn active_state_tracks_locked_flag() {
        let state = LockApiState::default();

        assert!(!state.active());
        state.set_active(true);
        assert!(state.active());
        state.set_active(false);
        assert!(!state.active());
    }
}
