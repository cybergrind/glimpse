mod agents;
mod app;
mod applets;
mod components;
mod dbus;
mod panels;
mod prompts;
mod theme;

use relm4::{RELM_THREADS, RelmApp};
use tracing_subscriber::EnvFilter;

pub use glimpse_core::{compositors, services};

use crate::{
    app::{App, AppInit},
    compositors::detect_compositor,
};
use glimpse_core::Config;
use glimpse_core::dbus::Dbus;

fn register_resources() {
    gio::resources_register_include!("glimpse-shell.gresource")
        .expect("failed to register embedded resources");
}

fn main() -> anyhow::Result<()> {
    if let Some(output) = version_output(std::env::args()) {
        println!("{output}");
        return Ok(());
    }

    let filter = EnvFilter::try_from_env("GLIMPSE_LOG_LEVEL")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // must be set before relm4's runtime is first touched.
    let threads = std::env::var("GLIMPSE_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);
    RELM_THREADS.set(threads).ok();

    let config = Config::autodetect();
    if let Some(compositor) = detect_compositor() {
        tracing::info!(compositor = compositor.name(), "detected compositor");
    } else {
        tracing::warn!("unsupported compositor");
    }

    let dbus = Dbus::connect()?;

    let app_id = std::env::var("GLIMPSE_SHELL_APP_ID").unwrap_or("me.aresa.GlimpseShell".into());
    let app = RelmApp::new(app_id.as_str());

    register_resources();
    app.with_args(vec![]).run::<App>(AppInit { config, dbus });

    Ok(())
}

fn version_output<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .skip(1)
        .any(|arg| matches!(arg.as_ref(), "--version" | "-V"))
        .then(|| format!("glimpse-shell {}", env!("CARGO_PKG_VERSION")))
}

#[cfg(test)]
mod tests {
    use super::version_output;

    #[test]
    fn version_output_uses_cargo_package_version_for_long_flag() {
        assert_eq!(
            version_output(["glimpse-shell", "--version"]),
            Some(format!("glimpse-shell {}", env!("CARGO_PKG_VERSION")))
        );
    }

    #[test]
    fn version_output_uses_cargo_package_version_for_short_flag() {
        assert_eq!(
            version_output(["glimpse-shell", "-V"]),
            Some(format!("glimpse-shell {}", env!("CARGO_PKG_VERSION")))
        );
    }

    #[test]
    fn version_output_is_absent_without_flag() {
        assert_eq!(version_output(["glimpse-shell"]), None);
    }
}
