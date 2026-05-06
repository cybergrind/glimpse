use glimpse_core::services::keyboard::KeyboardLayout;

pub fn layout_label(layout: &KeyboardLayout) -> String {
    layout.label.clone()
}

pub fn layout_tooltip(layout: &KeyboardLayout) -> String {
    layout.name.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_label_uses_normalized_service_label() {
        let layout = KeyboardLayout {
            index: 0,
            name: "English (US)".into(),
            code: "EN".into(),
            label: "🇺🇸".into(),
        };

        assert_eq!(layout_label(&layout), "🇺🇸");
    }

    #[test]
    fn layout_tooltip_uses_layout_name() {
        let layout = KeyboardLayout {
            index: 0,
            name: "English (US)".into(),
            code: "EN".into(),
            label: "🇺🇸".into(),
        };

        assert_eq!(layout_tooltip(&layout), "English (US)");
    }
}
