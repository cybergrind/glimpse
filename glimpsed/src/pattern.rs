/// A parsed topic pattern supporting `*` (one segment) and `**` (any depth) wildcards.
///
/// Topics and patterns are dot-separated: `battery.status`, `bluetooth.device.AA:BB`.
/// - `*` matches exactly one segment
/// - `**` matches zero or more segments
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pattern {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Segment {
    Literal(String),
    /// Matches exactly one segment.
    Single,
    /// Matches zero or more segments.
    Glob,
}

impl Pattern {
    pub fn parse(s: &str) -> Self {
        let mut segments: Vec<Segment> = s
            .split('.')
            .map(|seg| match seg {
                "**" => Segment::Glob,
                "*" => Segment::Single,
                _ => Segment::Literal(seg.to_owned()),
            })
            .collect();
        // Collapse consecutive globs to prevent exponential backtracking.
        segments.dedup_by(|a, b| matches!((a, b), (Segment::Glob, Segment::Glob)));
        Self { segments }
    }

    /// Returns the first literal segment, used as provider name for routing.
    pub fn provider_name(&self) -> Option<&str> {
        match self.segments.first() {
            Some(Segment::Literal(s)) if !s.is_empty() => Some(s),
            _ => None,
        }
    }

    pub fn matches(&self, topic: &str) -> bool {
        let topic_segments: Vec<&str> = topic.split('.').collect();
        matches_recursive(&self.segments, &topic_segments)
    }
}

fn matches_recursive(pattern: &[Segment], topic: &[&str]) -> bool {
    match (pattern, topic) {
        ([], []) => true,
        ([], [_, ..]) => false,
        ([Segment::Glob], _) => true,
        ([Segment::Glob, rest @ ..], _) => {
            for i in 0..=topic.len() {
                if matches_recursive(rest, &topic[i..]) {
                    return true;
                }
            }
            false
        }
        ([_, ..], []) => false,
        ([Segment::Literal(lit), rest @ ..], [first, remaining @ ..]) => {
            lit == first && matches_recursive(rest, remaining)
        }
        ([Segment::Single, rest @ ..], [_, remaining @ ..]) => matches_recursive(rest, remaining),
    }
}

impl std::fmt::Display for Pattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for seg in &self.segments {
            if !first {
                f.write_str(".")?;
            }
            first = false;
            match seg {
                Segment::Literal(l) => f.write_str(l)?,
                Segment::Single => f.write_str("*")?,
                Segment::Glob => f.write_str("**")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        let p = Pattern::parse("battery.status");
        assert!(p.matches("battery.status"));
        assert!(!p.matches("battery.devices"));
        assert!(!p.matches("battery"));
        assert!(!p.matches("battery.status.extra"));
    }

    #[test]
    fn single_wildcard() {
        let p = Pattern::parse("bluetooth.*");
        assert!(p.matches("bluetooth.devices"));
        assert!(p.matches("bluetooth.status"));
        assert!(!p.matches("bluetooth.device.AA:BB"));
        assert!(!p.matches("bluetooth"));
    }

    #[test]
    fn glob_wildcard() {
        let p = Pattern::parse("bluetooth.**");
        assert!(p.matches("bluetooth"));
        assert!(p.matches("bluetooth.devices"));
        assert!(p.matches("bluetooth.device.AA:BB"));
        assert!(p.matches("bluetooth.device.AA:BB.name"));
        assert!(!p.matches("audio.outputs"));
    }

    #[test]
    fn glob_in_middle() {
        let p = Pattern::parse("audio.**.volume");
        assert!(p.matches("audio.volume"));
        assert!(p.matches("audio.output.volume"));
        assert!(p.matches("audio.output.48.volume"));
        assert!(!p.matches("audio.output.48.mute"));
    }

    #[test]
    fn single_wildcard_in_middle() {
        let p = Pattern::parse("audio.output.*.volume");
        assert!(p.matches("audio.output.48.volume"));
        assert!(!p.matches("audio.output.volume"));
        assert!(!p.matches("audio.output.48.49.volume"));
    }

    #[test]
    fn bare_glob() {
        let p = Pattern::parse("**");
        assert!(p.matches("anything"));
        assert!(p.matches("a.b.c"));
    }

    #[test]
    fn consecutive_globs_collapsed() {
        let p = Pattern::parse("a.**.**.b");
        // Should behave the same as "a.**.b"
        assert!(p.matches("a.b"));
        assert!(p.matches("a.x.b"));
        assert!(p.matches("a.x.y.b"));
        assert_eq!(p.to_string(), "a.**.b");
    }

    #[test]
    fn provider_name() {
        assert_eq!(
            Pattern::parse("battery.status").provider_name(),
            Some("battery")
        );
        assert_eq!(
            Pattern::parse("bluetooth.**").provider_name(),
            Some("bluetooth")
        );
        assert_eq!(Pattern::parse("*.status").provider_name(), None);
        assert_eq!(Pattern::parse("**").provider_name(), None);
    }

    #[test]
    fn display_roundtrip() {
        let patterns = ["battery.status", "bluetooth.**", "audio.output.*.volume"];
        for s in patterns {
            assert_eq!(Pattern::parse(s).to_string(), s);
        }
    }

    #[test]
    fn no_match_different_prefix() {
        let p = Pattern::parse("battery.**");
        assert!(!p.matches("power.profiles"));
        assert!(!p.matches("audio.outputs"));
    }

    #[test]
    fn leading_trailing_dots() {
        // Leading dot produces an empty-string literal segment.
        let p = Pattern::parse(".foo");
        assert!(p.matches(".foo"));
        assert!(!p.matches("foo"));

        // Trailing dot.
        let p = Pattern::parse("foo.");
        assert!(p.matches("foo."));
        assert!(!p.matches("foo"));
    }

    #[test]
    fn empty_provider_name() {
        assert_eq!(Pattern::parse("").provider_name(), None);
        assert_eq!(Pattern::parse(".foo").provider_name(), None);
    }
}
