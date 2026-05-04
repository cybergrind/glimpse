pub mod model;
mod service;

pub(crate) use service::NotificationServerDispatcher;
pub use service::{NotificationsHandle, NotificationsService};
