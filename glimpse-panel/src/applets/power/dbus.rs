use futures_util::StreamExt;
use tokio::sync::mpsc;

use super::applet::PowerCommand;

pub(super) enum PowerAction {
    SetProfile(String),
    Suspend,
    Hibernate,
    Reboot,
    PowerOff,
}

pub(super) async fn monitor_battery(tx: mpsc::Sender<PowerCommand>) {
    if let Err(e) = try_monitor_battery(tx).await {
        tracing::error!("battery monitoring failed: {e}");
    }
}

async fn try_monitor_battery(tx: mpsc::Sender<PowerCommand>) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;

    let upower = zbus::Proxy::new(
        &conn,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower",
        "org.freedesktop.UPower",
    )
    .await?;

    let devices: Vec<zbus::zvariant::OwnedObjectPath> =
        upower.call("EnumerateDevices", &()).await?;

    let mut battery_path = None;
    for path in &devices {
        let dev = zbus::Proxy::new(
            &conn,
            "org.freedesktop.UPower",
            path.as_str(),
            "org.freedesktop.UPower.Device",
        )
        .await?;
        if dev.get_property::<u32>("Type").await.unwrap_or(0) == 2 {
            battery_path = Some(path.clone());
            break;
        }
    }

    let Some(bp) = battery_path else {
        tx.send(PowerCommand::NoBattery).await.ok();
        return Ok(());
    };

    let bat = zbus::Proxy::new(
        &conn,
        "org.freedesktop.UPower",
        bp.as_str(),
        "org.freedesktop.UPower.Device",
    )
    .await?;

    tx.send(read_battery_state(&bat).await).await.ok();

    let props = zbus::fdo::PropertiesProxy::builder(&conn)
        .destination("org.freedesktop.UPower")?
        .path(bp.as_str())?
        .build()
        .await?;
    let mut stream = props.receive_properties_changed().await?;

    while stream.next().await.is_some() {
        tx.send(read_battery_state(&bat).await).await.ok();
    }

    Ok(())
}

async fn read_battery_state(bat: &zbus::Proxy<'_>) -> PowerCommand {
    let pct = bat.get_property::<f64>("Percentage").await.unwrap_or(0.0) as u8;
    let state = bat.get_property::<u32>("State").await.unwrap_or(2);
    let icon = bat
        .get_property::<String>("IconName")
        .await
        .unwrap_or_else(|_| "battery-missing-symbolic".to_string());
    PowerCommand::BatteryUpdate {
        percentage: pct,
        charging: state == 1,
        icon_name: icon,
    }
}

pub(super) async fn monitor_profiles(tx: mpsc::Sender<PowerCommand>) {
    if let Err(e) = try_monitor_profiles(tx).await {
        tracing::warn!("profile monitoring unavailable: {e}");
    }
}

async fn try_monitor_profiles(tx: mpsc::Sender<PowerCommand>) -> zbus::Result<()> {
    use std::collections::HashMap;
    use zbus::zvariant::OwnedValue;

    let conn = zbus::Connection::system().await?;

    let proxy = zbus::Proxy::new(
        &conn,
        "net.hadess.PowerProfiles",
        "/net/hadess/PowerProfiles",
        "net.hadess.PowerProfiles",
    )
    .await?;

    let active: String = proxy
        .get_property("ActiveProfile")
        .await
        .unwrap_or_default();
    let raw: Vec<HashMap<String, OwnedValue>> =
        proxy.get_property("Profiles").await.unwrap_or_default();
    tx.send(PowerCommand::ProfilesUpdate {
        profiles: extract_profile_names(&raw),
        active,
    })
    .await
    .ok();

    let props = zbus::fdo::PropertiesProxy::builder(&conn)
        .destination("net.hadess.PowerProfiles")?
        .path("/net/hadess/PowerProfiles")?
        .build()
        .await?;
    let mut stream = props.receive_properties_changed().await?;

    while stream.next().await.is_some() {
        let active: String = proxy
            .get_property("ActiveProfile")
            .await
            .unwrap_or_default();
        let raw: Vec<HashMap<String, OwnedValue>> =
            proxy.get_property("Profiles").await.unwrap_or_default();
        tx.send(PowerCommand::ProfilesUpdate {
            profiles: extract_profile_names(&raw),
            active,
        })
        .await
        .ok();
    }

    Ok(())
}

fn extract_profile_names(
    raw: &[std::collections::HashMap<String, zbus::zvariant::OwnedValue>],
) -> Vec<String> {
    use zbus::zvariant::Value;
    raw.iter()
        .filter_map(|d| {
            d.get("Profile").and_then(|v| match &**v {
                Value::Str(s) => Some(s.to_string()),
                Value::Value(inner) => {
                    if let Value::Str(s) = inner.as_ref() {
                        Some(s.to_string())
                    } else {
                        None
                    }
                }
                _ => None,
            })
        })
        .collect()
}

pub(super) async fn handle_action(conn: &zbus::Connection, action: PowerAction) {
    match action {
        PowerAction::SetProfile(p) => {
            if let Err(e) = set_power_profile(conn, &p).await {
                tracing::warn!("set profile failed: {e}");
            }
        }
        PowerAction::Suspend => logind_call(conn, "Suspend").await,
        PowerAction::Hibernate => logind_call(conn, "Hibernate").await,
        PowerAction::Reboot => logind_call(conn, "Reboot").await,
        PowerAction::PowerOff => logind_call(conn, "PowerOff").await,
    }
}

async fn set_power_profile(conn: &zbus::Connection, profile: &str) -> zbus::fdo::Result<()> {
    let proxy: zbus::Proxy<'_> = zbus::Proxy::new(
        conn,
        "net.hadess.PowerProfiles",
        "/net/hadess/PowerProfiles",
        "net.hadess.PowerProfiles",
    )
    .await
    .map_err(zbus::fdo::Error::from)?;
    proxy.set_property("ActiveProfile", profile).await
}

async fn logind_call(conn: &zbus::Connection, method: &str) {
    let result: zbus::Result<()> = async {
        let proxy: zbus::Proxy<'_> = zbus::Proxy::new(
            conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await?;
        proxy.call::<_, (bool,), ()>(method, &(false,)).await
    }
    .await;
    if let Err(e) = result {
        tracing::warn!("logind {method} failed: {e}");
    }
}
