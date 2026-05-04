use std::collections::HashMap;

use glimpse_core::compositors::{KeyboardLayout, keyboard_layout_code};

pub fn layout_label(layout: &KeyboardLayout, labels: &HashMap<String, String>) -> String {
    let code = keyboard_layout_code(&layout.name);
    labels
        .get(&layout.name)
        .or_else(|| labels.get(&layout.name.to_lowercase()))
        .or_else(|| labels.get(&code.to_lowercase()))
        .or_else(|| labels.get(&code))
        .cloned()
        .unwrap_or(code)
}

pub fn layout_tooltip(layout: &KeyboardLayout) -> String {
    layout.name.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_label_prefers_exact_name_override() {
        let layout = KeyboardLayout {
            index: 0,
            name: "English (US)".into(),
        };
        let labels = HashMap::from([("English (US)".into(), "🇺🇸".into())]);

        assert_eq!(layout_label(&layout, &labels), "🇺🇸");
    }

    #[test]
    fn layout_label_accepts_code_override() {
        let layout = KeyboardLayout {
            index: 0,
            name: "us".into(),
        };
        let labels = HashMap::from([("us".into(), "🇺🇸".into())]);

        assert_eq!(layout_label(&layout, &labels), "🇺🇸");
    }

    #[test]
    fn layout_label_falls_back_to_code() {
        let layout = KeyboardLayout {
            index: 0,
            name: "Polish".into(),
        };

        assert_eq!(layout_label(&layout, &HashMap::new()), "PL");
    }
}
