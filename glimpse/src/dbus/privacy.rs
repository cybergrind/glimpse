pub const PORTAL_DESKTOP_BUS: &str = "org.freedesktop.portal.Desktop";
pub const PORTAL_SESSION_INTERFACE: &str = "org.freedesktop.portal.Session";
pub const MUTTER_SCREENCAST_BUS: &str = "org.gnome.Mutter.ScreenCast";
pub const MUTTER_SCREENCAST_SESSION_INTERFACE: &str = "org.gnome.Mutter.ScreenCast.Session";

#[zbus::proxy(
    interface = "org.freedesktop.portal.Session",
    default_service = "org.freedesktop.portal.Desktop",
    assume_defaults = true
)]
pub trait PortalSession {
    fn close(&self) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.gnome.Mutter.ScreenCast.Session",
    default_service = "org.gnome.Mutter.ScreenCast",
    assume_defaults = true
)]
pub trait MutterScreenCastSession {
    fn stop(&self) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_proxy_constants_match_expected_interface_names() {
        assert_eq!(PORTAL_DESKTOP_BUS, "org.freedesktop.portal.Desktop");
        assert_eq!(PORTAL_SESSION_INTERFACE, "org.freedesktop.portal.Session");
        assert_eq!(MUTTER_SCREENCAST_BUS, "org.gnome.Mutter.ScreenCast");
        assert_eq!(
            MUTTER_SCREENCAST_SESSION_INTERFACE,
            "org.gnome.Mutter.ScreenCast.Session"
        );
    }
}
