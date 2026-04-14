use glimpse::notifications::NotificationsCommand;

mod activation;
mod applet;
mod components;
mod popover;
mod popup;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationActionCommand {
    Dismiss {
        id: u32,
    },
    DismissAll,
    InvokeAction {
        id: u32,
        action_key: String,
        activation_token: Option<String>,
    },
    SetDnd(bool),
}

impl NotificationActionCommand {
    pub(crate) fn into_service_command(self) -> NotificationsCommand {
        match self {
            Self::Dismiss { id } => NotificationsCommand::Dismiss { id },
            Self::DismissAll => NotificationsCommand::DismissAll,
            Self::InvokeAction {
                id,
                action_key,
                activation_token,
            } => NotificationsCommand::InvokeAction {
                id,
                action_key,
                activation_token,
            },
            Self::SetDnd(enabled) => NotificationsCommand::SetDnd(enabled),
        }
    }
}

pub use applet::{Notifications, NotificationsInit, NotificationsMsg};
pub use glimpse::config::NotificationsConfig;
pub use popup::{NotificationPopup, NotificationPopupInit, NotificationPopupInput};
