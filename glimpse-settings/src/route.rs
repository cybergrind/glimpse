use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route(String);

impl Route {
    pub fn parse(value: &str) -> Option<Self> {
        let trimmed = value.trim().trim_matches('/');
        if trimmed.is_empty() {
            return None;
        }

        let mut segments = trimmed.split('/').filter(|segment| !segment.is_empty());
        let head = canonical_head(segments.next()?);
        let mut canonical = head.to_string();

        for segment in segments {
            canonical.push('/');
            canonical.push_str(segment);
        }

        Some(Self(canonical))
    }

    pub fn head(&self) -> &str {
        self.0.split('/').next().unwrap_or_default()
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn canonical_head(segment: &str) -> &str {
    match segment {
        "audio" => "sound",
        "display" => "displays",
        "date-time" => "date-time-locale",
        "locale" => "date-time-locale",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::Route;

    #[test]
    fn parses_top_level_sound_route() {
        let route = Route::parse("sound").expect("sound route should parse");

        assert_eq!(route.to_string(), "sound");
    }

    #[test]
    fn normalizes_aliases_to_canonical_routes() {
        let route = Route::parse("audio").expect("audio alias should parse");

        assert_eq!(route.to_string(), "sound");
    }

    #[test]
    fn preserves_subpages_in_canonical_form() {
        let route = Route::parse("bluetooth/adapters").expect("subroute should parse");

        assert_eq!(route.to_string(), "bluetooth/adapters");
    }
}
