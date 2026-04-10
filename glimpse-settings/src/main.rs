use glimpse_settings::startup::StartupRequest;

mod app;

fn main() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("GLIMPSE_LOG_LEVEL")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,gtk4=warn,relm4=warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let request = StartupRequest::from_args(std::env::args());
    app::run(request);
}
