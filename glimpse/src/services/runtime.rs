use crate::{
    config::Config,
    services::{
        control::ControlEvent,
        framework::{RunningService, ServiceEvent, spawn_service},
        location::service::{
            LocationCommand, LocationConfig, LocationService, LocationServiceHandle,
        },
    },
};

#[derive(Clone)]
pub struct Service {
    pub location: LocationServiceHandle,
}

pub struct ServiceRuntime {
    pub handle: Service,

    location: RunningService<LocationCommand>,
}

impl ServiceRuntime {
    pub fn new(
        config: &Config,
         _session_dbus: zbus::Connection,
         _system_dbus: zbus::Connection,
    ) -> Self {
        let (location, location_handle) = LocationService::new(LocationConfig {
            source: config.location.source.clone(),
            latitude: config.location.latitude,
            longitude: config.location.longitude,
        });

        let location_service = spawn_service("location", move |cancel, control| {
            location.run(cancel, control)
        });

        Self {
            location: location_service,
            handle: Service {
                location: location_handle,
            },
        }
    }

    pub fn reconfigure(&self, config: &Config) {
        send_control_event(&self.location, ControlEvent::Reconfigure(config.clone()));
    }

    pub async fn shutdown(self) {
        send_control_event(&self.location, ControlEvent::Shutdown);
        self.location.shutdown().await;
    }
}

fn send_control_event<C: Send + 'static>(service: &RunningService<C>, event: ControlEvent) {
    if let Err(e) = service.try_send(ServiceEvent::Control(event)) {
        tracing::warn!(
            "failed to send control event to service {}: {:?}",
            service.name,
            e
        )
    }
}
