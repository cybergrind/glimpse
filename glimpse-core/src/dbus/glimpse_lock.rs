pub const GLIMPSE_LOCK_BUS_NAME: &str = "me.aresa.GlimpseLock";
pub const GLIMPSE_LOCK_OBJECT_PATH: &str = "/me/aresa/GlimpseLock";

#[zbus::proxy(
    interface = "me.aresa.GlimpseLock",
    default_service = "me.aresa.GlimpseLock",
    default_path = "/me/aresa/GlimpseLock"
)]
pub trait GlimpseLock {
    #[zbus(name = "Lock")]
    fn lock(&self) -> zbus::Result<()>;

    #[zbus(name = "GetActive")]
    fn get_active(&self) -> zbus::Result<bool>;

    #[zbus(name = "GetActiveTime")]
    fn get_active_time(&self) -> zbus::Result<u32>;
}
