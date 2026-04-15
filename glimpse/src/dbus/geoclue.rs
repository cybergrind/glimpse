use zbus::zvariant::OwnedObjectPath;

#[zbus::proxy(
    interface = "org.freedesktop.GeoClue2.Manager",
    default_service = "org.freedesktop.GeoClue2",
    default_path = "/org/freedesktop/GeoClue2/Manager"
)]
pub trait GeoClueManager {
    fn create_client(&self) -> zbus::Result<OwnedObjectPath>;
    fn delete_client(&self, client: OwnedObjectPath) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.GeoClue2.Client",
    default_service = "org.freedesktop.GeoClue2"
)]
pub trait GeoClueClient {
    fn start(&self) -> zbus::Result<()>;
    fn stop(&self) -> zbus::Result<()>;

    #[zbus(property, name = "DesktopId")]
    fn set_desktop_id(&self, value: &str) -> zbus::Result<()>;
    #[zbus(property, name = "RequestedAccuracyLevel")]
    fn set_requested_accuracy_level(&self, value: u32) -> zbus::Result<()>;
    #[zbus(property)]
    fn location(&self) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.GeoClue2.Location",
    default_service = "org.freedesktop.GeoClue2"
)]
pub trait GeoClueLocation {
    #[zbus(property, name = "Latitude")]
    fn latitude(&self) -> zbus::Result<f64>;
    #[zbus(property, name = "Longitude")]
    fn longitude(&self) -> zbus::Result<f64>;
}
