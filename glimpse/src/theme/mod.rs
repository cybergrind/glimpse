pub mod protocol;

pub mod provider {
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct ThemePreferenceBackend;

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct ThemeProvider;
}

pub mod service {
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct ThemeServiceHandle;
}

pub use protocol::{
    ThemeCommand, ThemeHealth, ThemePreference, ThemePreferenceSnapshot, ThemeSource, ThemeState,
};
pub use provider::{ThemePreferenceBackend, ThemeProvider};
pub use service::ThemeServiceHandle;
