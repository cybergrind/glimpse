pub mod model;
mod persistence;
mod service;

pub(crate) use service::NotificationServerDispatcher;
pub use service::{NotificationsHandle, NotificationsService};
