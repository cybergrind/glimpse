use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    config::Config,
    services::{
        control::ControlEvent,
        location::service::{LocationConfig, LocationService, LocationServiceHandle},
    },
};

#[derive(Clone)]
pub struct ServiceHandle {
    pub location: LocationServiceHandle,
}

struct RunningService {
    task: tokio::task::JoinHandle<()>,
    cancel: CancellationToken,
    control_tx: mpsc::Sender<ControlEvent>,
}

impl RunningService {
    fn spawn<F, Fut, E>(service_name: &'static str, run: F) -> Self
    where
        F: FnOnce(CancellationToken, mpsc::Receiver<ControlEvent>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), E>> + Send + 'static,
        E: std::fmt::Debug,
    {
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let (control_tx, control_rx) = mpsc::channel(16);
        let task = tokio::spawn(async move {
            if let Err(error) = run(task_cancel, control_rx).await {
                tracing::warn!(error=?error, "{} service stopped with error", service_name);
            };
        });

        Self {
            task,
            cancel,
            control_tx,
        }
    }

    fn reconfigure(&self, event: ControlEvent) {
        let _ = self.control_tx.try_send(event);
    }

    async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.task.await;
    }
}

pub struct ServiceRuntime {
    pub handle: ServiceHandle,

    running_services: Vec<RunningService>,
}

impl ServiceRuntime {
    pub fn new(
        config: &Config,
        session_dbus: zbus::Connection,
        system_dbus: zbus::Connection,
    ) -> Self {
        let mut running_services = vec![];
        let (location, location_handle) = LocationService::new(LocationConfig {
            source: config.location.source.clone(),
            latitude: config.location.latitude,
            longitude: config.location.longitude,
        });
        running_services.push(RunningService::spawn("location", move |cancel, control| {
            location.run(cancel, control)
        }));

        Self {
            running_services,
            handle: ServiceHandle {
                location: location_handle,
            },
        }
    }

    pub fn reconfigure(&self, config: &Config) {
        for running in &self.running_services {
            running.reconfigure(ControlEvent::Reconfigure(config.clone()));
        }
    }

    pub async fn shutdown(self) {
        for running in self.running_services {
            running.shutdown().await;
        }
    }
}
