use crate::{
    config::Config,
    services::{
        control::ControlEvent,
        framework::{RunningService, ServiceEvent, spawn_service},
        location::service::{LocationCommand, LocationService, LocationServiceHandle},
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
        let (location, location_handle) = LocationService::new();
        let location_service = spawn_service("location", move |control, cancel| {
            location.run(control, cancel)
        });

        let instance = Self {
            location: location_service,
            handle: Service {
                location: location_handle,
            },
        };
        instance.reconfigure(config);
        instance
    }

    pub fn reconfigure(&self, config: &Config) {
        send_control_event(&self.location, ControlEvent::Configure(config.clone()));
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
