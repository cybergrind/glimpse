use zbus::zvariant::ObjectPath;

const AGENT_PATH: &str = "/org/bluez/glimpse/agent";

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum BluezError {
    Rejected(String),
}

struct BluetoothAgent;

#[zbus::interface(name = "org.bluez.Agent1")]
impl BluetoothAgent {
    fn release(&self) {
        tracing::info!("bluetooth-agent: released");
    }

    async fn request_default(&self) {
        tracing::info!("bluetooth-agent: set as default agent");
    }

    async fn request_confirmation(
        &self,
        device: ObjectPath<'_>,
        passkey: u32,
    ) -> zbus::fdo::Result<()> {
        tracing::info!(
            device = device.as_str(),
            passkey,
            "bluetooth-agent: auto-confirming pairing"
        );
        Ok(())
    }

    async fn authorize_service(&self, device: ObjectPath<'_>, uuid: &str) -> zbus::fdo::Result<()> {
        tracing::info!(
            device = device.as_str(),
            uuid,
            "bluetooth-agent: authorizing service"
        );
        Ok(())
    }

    async fn request_passkey(&self, device: ObjectPath<'_>) -> Result<u32, BluezError> {
        let dev = device.as_str().to_owned();
        tracing::info!(
            device = dev,
            "bluetooth-agent: requesting passkey via dialog"
        );
        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new("zenity")
                .args([
                    "--entry",
                    "--title=Bluetooth Pairing",
                    &format!("--text=Enter passkey for {}", dev),
                ])
                .output()
        })
        .await;

        let passkey = result
            .ok()
            .and_then(|r| r.ok())
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u32>().ok());

        match passkey {
            Some(pk) => {
                tracing::info!("bluetooth-agent: passkey entered");
                Ok(pk)
            }
            None => {
                tracing::info!("bluetooth-agent: passkey dialog cancelled");
                Err(BluezError::Rejected("cancelled by user".into()))
            }
        }
    }

    async fn display_passkey(&self, device: ObjectPath<'_>, passkey: u32, _entered: u16) {
        tracing::info!(
            device = device.as_str(),
            passkey,
            "bluetooth-agent: display passkey"
        );
    }

    async fn display_pin_code(
        &self,
        device: ObjectPath<'_>,
        pincode: &str,
    ) -> zbus::fdo::Result<()> {
        tracing::info!(
            device = device.as_str(),
            pincode,
            "bluetooth-agent: display pin code"
        );
        Ok(())
    }

    async fn request_pin_code(&self, device: ObjectPath<'_>) -> Result<String, BluezError> {
        let dev = device.as_str().to_owned();
        tracing::info!(device = dev, "bluetooth-agent: requesting PIN via dialog");
        let result = tokio::task::spawn_blocking(move || {
            std::process::Command::new("zenity")
                .args([
                    "--entry",
                    "--title=Bluetooth Pairing",
                    &format!("--text=Enter PIN for {}", dev),
                ])
                .output()
        })
        .await;

        let pin = result
            .ok()
            .and_then(|r| r.ok())
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        match pin {
            Some(p) => {
                tracing::info!("bluetooth-agent: PIN entered");
                Ok(p)
            }
            None => {
                tracing::info!("bluetooth-agent: PIN dialog cancelled");
                Err(BluezError::Rejected("cancelled by user".into()))
            }
        }
    }

    fn cancel(&self) {
        tracing::info!("bluetooth-agent: pairing cancelled");
    }
}

pub async fn run(cancel: tokio_util::sync::CancellationToken) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;

    conn.object_server().at(AGENT_PATH, BluetoothAgent).await?;

    let agent_mgr =
        zbus::Proxy::new(&conn, "org.bluez", "/org/bluez", "org.bluez.AgentManager1").await?;

    let path = ObjectPath::try_from(AGENT_PATH)?;

    agent_mgr
        .call::<_, _, ()>("RegisterAgent", &(&path, "KeyboardDisplay"))
        .await
        .map_err(|e| anyhow::anyhow!("failed to register bluetooth agent: {e}"))?;

    // Try to become the default agent
    let _ = agent_mgr
        .call::<_, _, ()>("RequestDefaultAgent", &(&path,))
        .await;

    tracing::info!("bluetooth-agent: registered");

    cancel.cancelled().await;

    let _ = agent_mgr
        .call::<_, _, ()>("UnregisterAgent", &(&path,))
        .await;
    tracing::info!("bluetooth-agent: unregistered");

    Ok(())
}
