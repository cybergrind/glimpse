use std::rc::Rc;

use glimpse::notifications::NotificationEntry;

use super::NotificationActionCommand;

pub mod hero;
pub mod group_header;
pub mod list;
pub mod row;
pub mod stack;
pub mod stack_hint;

pub type NotifData = NotificationEntry;
pub type NotificationCommandEmitter = Rc<dyn Fn(NotificationActionCommand)>;
pub type StackToggleEmitter = Rc<dyn Fn(String)>;
