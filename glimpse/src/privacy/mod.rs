pub mod protocol;

pub mod service {
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct PrivacyServiceHandle;
}

pub use service::PrivacyServiceHandle;
