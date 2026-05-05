use glimpse_core::services::clipboard::{ClipboardEntry, ClipboardEntryKind, State};

pub const DEFAULT_LABEL_FORMAT: &str = "";
pub const DEFAULT_TOOLTIP_FORMAT: &str = "{count} clipboard items";

pub fn icon_name(_state: &State) -> &'static str {
    "edit-paste-symbolic"
}

pub fn label(format: &str, state: &State) -> String {
    render(format, state)
}

pub fn tooltip(format: &str, state: &State) -> String {
    if !state.available {
        return "Clipboard unavailable".into();
    }
    render(format, state)
}

pub fn hero_subtitle(state: &State) -> String {
    if !state.available {
        return "Unavailable".into();
    }
    count_label(state.history.len())
}

pub fn count_label(count: usize) -> String {
    match count {
        0 => "No items".into(),
        1 => "1 item".into(),
        count => format!("{count} items"),
    }
}

pub fn entry_icon(entry: &ClipboardEntry) -> &'static str {
    match entry.kind {
        ClipboardEntryKind::Text => "text-x-generic-symbolic",
        ClipboardEntryKind::Html => "text-x-generic-symbolic",
        ClipboardEntryKind::Image => "image-x-generic-symbolic",
        ClipboardEntryKind::Files => "folder-documents-symbolic",
        ClipboardEntryKind::Other => "package-x-generic-symbolic",
    }
}

fn render(format: &str, state: &State) -> String {
    format
        .replace("{count}", &state.history.len().to_string())
        .replace(
            "{state}",
            if state.available {
                "available"
            } else {
                "unavailable"
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_count_and_state_placeholders() {
        let state = State {
            available: true,
            ..State::default()
        };

        assert_eq!(label("{count}:{state}", &state), "0:available");
    }
}
