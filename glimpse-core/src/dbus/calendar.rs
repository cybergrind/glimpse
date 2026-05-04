use zbus::zvariant::{OwnedObjectPath, OwnedValue};

pub const CALENDAR_SERVER_DEST: &str = "org.gnome.Shell.CalendarServer";
pub const CALENDAR_SERVER_PATH: &str = "/org/gnome/Shell/CalendarServer";
pub const CALENDAR_SERVER_IFACE: &str = "org.gnome.Shell.CalendarServer";
pub const SOURCE_IFACE: &str = "org.gnome.evolution.dataserver.Source";
pub const SIGNAL_EVENTS_ADDED_OR_UPDATED: &str = "EventsAddedOrUpdated";
pub const SIGNAL_EVENTS_REMOVED: &str = "EventsRemoved";

pub type ManagedObjects = std::collections::HashMap<
    OwnedObjectPath,
    std::collections::HashMap<String, std::collections::HashMap<String, OwnedValue>>,
>;

pub type CalendarServerEventPayload = (
    String,
    String,
    i64,
    i64,
    std::collections::HashMap<String, OwnedValue>,
);

#[zbus::proxy(
    interface = "org.gnome.Shell.CalendarServer",
    default_service = "org.gnome.Shell.CalendarServer",
    default_path = "/org/gnome/Shell/CalendarServer"
)]
pub trait CalendarServer {
    #[zbus(name = "SetTimeRange")]
    fn set_time_range(&self, start: i64, end: i64, force_reload: bool) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.DBus.ObjectManager",
    default_service = "org.gnome.evolution.dataserver.Sources5",
    default_path = "/org/gnome/evolution/dataserver/SourceManager"
)]
pub trait EvolutionSourceManager {
    #[zbus(name = "GetManagedObjects")]
    fn get_managed_objects(&self) -> zbus::Result<ManagedObjects>;
}
