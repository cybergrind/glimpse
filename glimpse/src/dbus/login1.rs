use zbus::zvariant::OwnedObjectPath;

pub type Login1SessionEntry = (String, u32, String, String, OwnedObjectPath);

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
pub trait Login1Manager {
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
    fn lock_sessions(&self) -> zbus::Result<()>;
    fn terminate_session(&self, session_id: &str) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1"
)]
pub trait Login1Session {
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
}
