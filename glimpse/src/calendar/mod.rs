pub mod protocol;

pub mod service {
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct CalendarServiceHandle;
}

pub use service::CalendarServiceHandle;
