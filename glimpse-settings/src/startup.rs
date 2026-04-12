use crate::route::Route;

#[derive(Debug, Clone)]
pub struct StartupRequest {
    argv0: String,
    route: Route,
}

impl StartupRequest {
    pub fn from_args<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut args = args.into_iter();
        let argv0 = args
            .next()
            .map(|value| value.as_ref().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "glimpse-settings".to_string());
        let route = args
            .next()
            .and_then(|value| Route::parse(value.as_ref()))
            .unwrap_or_else(|| Route::parse("about").expect("default route should parse"));

        Self { argv0, route }
    }

    pub fn route(&self) -> &Route {
        &self.route
    }

    pub fn gtk_args(&self) -> [&str; 1] {
        [&self.argv0]
    }
}

#[cfg(test)]
mod tests {
    use crate::startup::StartupRequest;

    #[test]
    fn defaults_to_about_when_no_route_argument_is_present() {
        let request = StartupRequest::from_args(["glimpse-settings"]);

        assert_eq!(request.route().to_string(), "about");
        assert_eq!(request.gtk_args(), ["glimpse-settings"]);
    }

    #[test]
    fn uses_the_first_route_argument_when_present() {
        let request = StartupRequest::from_args(["glimpse-settings", "sound/output"]);

        assert_eq!(request.route().to_string(), "sound/output");
        assert_eq!(request.gtk_args(), ["glimpse-settings"]);
    }
}
