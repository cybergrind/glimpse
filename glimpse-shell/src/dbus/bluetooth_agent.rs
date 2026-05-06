use zbus::zvariant::ObjectPath;

use crate::agents::bluetooth::{
    BluetoothAgent, BluetoothPromptKind, BluetoothPromptReply, BluezError,
};

#[zbus::interface(name = "org.bluez.Agent1")]
impl BluetoothAgent {
    fn release(&self) {
        tracing::debug!("bluetooth-agent: released by bluez");
        tracing::info!("bluetooth-agent: released");
    }

    async fn request_confirmation(
        &self,
        device: ObjectPath<'_>,
        passkey: u32,
    ) -> Result<(), BluezError> {
        let device_path = device.as_str().to_owned();
        tracing::info!(
            device = device_path,
            passkey,
            "bluetooth-agent: confirmation requested"
        );
        tracing::debug!(
            device = device_path,
            prompt_kind = "confirm",
            "bluetooth-agent: confirmation prompt requested"
        );
        match self
            .request_reply(&device_path, BluetoothPromptKind::Confirm { passkey })
            .await?
        {
            BluetoothPromptReply::Confirm => Ok(()),
            BluetoothPromptReply::Cancel => Err(BluezError::Canceled("cancelled by user".into())),
            _ => Err(BluezError::Rejected("rejected by user".into())),
        }
    }

    async fn request_authorization(&self, device: ObjectPath<'_>) -> zbus::fdo::Result<()> {
        let device_path = device.as_str().to_owned();
        tracing::info!(
            device = device_path,
            "bluetooth-agent: authorizing pairing request"
        );
        tracing::debug!(
            device = device_path,
            prompt_kind = "authorize-pairing",
            "bluetooth-agent: authorization prompt requested"
        );
        match self
            .request_reply(&device_path, BluetoothPromptKind::AuthorizePairing)
            .await
        {
            Ok(BluetoothPromptReply::Confirm) => Ok(()),
            Ok(BluetoothPromptReply::Cancel) => {
                Err(zbus::fdo::Error::Failed("cancelled by user".into()))
            }
            Ok(_) => Err(zbus::fdo::Error::Failed("rejected by user".into())),
            Err(error) => Err(zbus::fdo::Error::Failed(error.to_string())),
        }
    }

    async fn authorize_service(&self, device: ObjectPath<'_>, uuid: &str) -> zbus::fdo::Result<()> {
        let device_path = device.as_str().to_owned();
        tracing::info!(
            device = device_path,
            uuid,
            "bluetooth-agent: authorizing service"
        );
        if self.trust_if_paired(&device_path).await? {
            tracing::info!(
                device = device_path,
                uuid,
                "bluetooth-agent: authorized service for paired device"
            );
            return Ok(());
        }

        tracing::debug!(
            device = device_path,
            uuid,
            prompt_kind = "authorize-service",
            "bluetooth-agent: service authorization prompt requested"
        );
        match self
            .request_reply(
                &device_path,
                BluetoothPromptKind::AuthorizeService { uuid: uuid.into() },
            )
            .await
        {
            Ok(BluetoothPromptReply::Confirm) => Ok(()),
            Ok(BluetoothPromptReply::Cancel) => {
                Err(zbus::fdo::Error::Failed("cancelled by user".into()))
            }
            Ok(_) => Err(zbus::fdo::Error::Failed("rejected by user".into())),
            Err(error) => Err(zbus::fdo::Error::Failed(error.to_string())),
        }
    }

    async fn request_passkey(&self, device: ObjectPath<'_>) -> Result<u32, BluezError> {
        let device_path = device.as_str().to_owned();
        tracing::debug!(
            device = device_path,
            prompt_kind = "request-passkey",
            "bluetooth-agent: passkey prompt requested"
        );
        self.request_passkey_reply(&device_path, BluetoothPromptKind::RequestPasskey, "passkey")
            .await
    }

    async fn display_passkey(&self, device: ObjectPath<'_>, passkey: u32, entered: u16) {
        let device_path = device.as_str().to_owned();
        tracing::debug!(
            device = device_path,
            entered,
            prompt_kind = "display-passkey",
            "bluetooth-agent: display passkey requested"
        );
        let prompt_id = self
            .publish_display_prompt(
                &device_path,
                BluetoothPromptKind::DisplayPasskey { passkey, entered },
            )
            .await;
        let Ok(prompt_id) = prompt_id else {
            tracing::warn!(
                device = device_path,
                "bluetooth-agent: display passkey prompt skipped because another prompt is active"
            );
            return;
        };
        tracing::info!(
            device = device_path,
            prompt_id = prompt_id.0,
            passkey,
            entered,
            "bluetooth-agent: display passkey"
        );
    }

    async fn display_pin_code(
        &self,
        device: ObjectPath<'_>,
        pincode: &str,
    ) -> zbus::fdo::Result<()> {
        let device_path = device.as_str().to_owned();
        tracing::debug!(
            device = device_path,
            pin_length = pincode.chars().count(),
            prompt_kind = "display-pin",
            "bluetooth-agent: display pin requested"
        );
        let prompt_id = self
            .publish_display_prompt(
                &device_path,
                BluetoothPromptKind::DisplayPin {
                    pincode: pincode.to_owned(),
                },
            )
            .await
            .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))?;
        tracing::info!(
            device = device_path,
            prompt_id = prompt_id.0,
            "bluetooth-agent: display pin code"
        );
        Ok(())
    }

    async fn request_pin_code(&self, device: ObjectPath<'_>) -> Result<String, BluezError> {
        let device_path = device.as_str().to_owned();
        tracing::debug!(
            device = device_path,
            prompt_kind = "request-pin",
            "bluetooth-agent: pin prompt requested"
        );
        self.request_string_reply(&device_path, BluetoothPromptKind::RequestPin, "pin")
            .await
    }

    fn cancel(&self) {
        tracing::debug!("bluetooth-agent: cancel requested by bluez");
        if self.cancel_prompt() {
            tracing::info!("bluetooth-agent: pairing cancelled");
        } else {
            tracing::info!("bluetooth-agent: pairing cancelled with no active prompt");
        }
    }
}
