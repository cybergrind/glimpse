use colored_json::ColorMode;

pub fn format_json(value: &serde_json::Value, color: bool, pretty: bool) -> String {
    if color {
        // colored_json always pretty-prints
        colored_json::to_colored_json(value, ColorMode::On).unwrap_or_else(|_| compact(value))
    } else if pretty {
        serde_json::to_string_pretty(value).unwrap_or_else(|_| compact(value))
    } else {
        compact(value)
    }
}

fn compact(value: &serde_json::Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}
