#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Stub,
    Appearance,
    Bluetooth,
    Displays,
    Power,
    Sound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSpec {
    pub route_head: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
    pub keywords: &'static [&'static str],
    pub kind: PageKind,
}

pub const PAGES: &[PageSpec] = &[
    PageSpec {
        route_head: "appearance",
        title: "Appearance",
        summary: "Theme mode, accent color, fonts, and text scaling.",
        keywords: &[
            "theme",
            "accent",
            "dark",
            "light",
            "fonts",
            "icon",
            "cursor",
            "scale",
        ],
        kind: PageKind::Appearance,
    },
    PageSpec {
        route_head: "displays",
        title: "Displays",
        summary: "Monitor layout, scale, refresh rate, and primary display.",
        keywords: &["display", "monitor", "resolution", "scale"],
        kind: PageKind::Displays,
    },
    PageSpec {
        route_head: "sound",
        title: "Sound",
        summary: "Outputs, inputs, volume, and default devices.",
        keywords: &["audio", "volume", "microphone", "speaker"],
        kind: PageKind::Sound,
    },
    PageSpec {
        route_head: "network",
        title: "Wi-Fi & Network",
        summary: "Wireless, wired, VPN, and connection details.",
        keywords: &["wifi", "wireless", "ethernet", "vpn", "internet"],
        kind: PageKind::Stub,
    },
    PageSpec {
        route_head: "bluetooth",
        title: "Bluetooth",
        summary: "Adapters, pairing, and connected devices.",
        keywords: &["adapter", "pairing", "device"],
        kind: PageKind::Bluetooth,
    },
    PageSpec {
        route_head: "power",
        title: "Power & Battery",
        summary: "Battery health, profiles, and sleep behavior.",
        keywords: &["battery", "energy", "suspend", "profile"],
        kind: PageKind::Power,
    },
    PageSpec {
        route_head: "keyboard",
        title: "Keyboard & Input",
        summary: "Layouts, shortcuts entry points, and pointing devices.",
        keywords: &["input", "touchpad", "mouse", "layout"],
        kind: PageKind::Stub,
    },
    PageSpec {
        route_head: "startup-applications",
        title: "Startup Applications",
        summary: "Manage user services and startup units.",
        keywords: &["startup", "autostart", "systemd", "services"],
        kind: PageKind::Stub,
    },
    PageSpec {
        route_head: "date-time-locale",
        title: "Date, Time & Locale",
        summary: "Timezone, regional formats, and clock preferences.",
        keywords: &["time", "date", "locale", "timezone", "region"],
        kind: PageKind::Stub,
    },
    PageSpec {
        route_head: "about",
        title: "About",
        summary: "System details, versions, and diagnostics.",
        keywords: &["system", "version", "diagnostics", "info"],
        kind: PageKind::Stub,
    },
];

pub fn find_by_route_head(route_head: &str) -> Option<&'static PageSpec> {
    PAGES.iter().find(|page| page.route_head == route_head)
}

pub fn search_pages(query: &str) -> Vec<&'static PageSpec> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return PAGES.iter().collect();
    }

    PAGES
        .iter()
        .filter(|page| {
            page.title.to_lowercase().contains(&query)
                || page.summary.to_lowercase().contains(&query)
                || page.keywords.iter().any(|keyword| keyword.contains(&query))
        })
        .collect()
}

pub fn sound_sections() -> &'static [(&'static str, &'static str)] {
    &[
        ("Output", "Choose where sound plays and set the default output device."),
        ("Input", "Select the active microphone and review capture levels."),
        ("Applications", "Adjust per-application playback volume and mute state."),
    ]
}

#[cfg(test)]
mod tests {
    use super::{PageKind, find_by_route_head, search_pages, sound_sections};

    #[test]
    fn finds_sound_page_by_route_head() {
        let page = find_by_route_head("sound").expect("sound page should exist");

        assert_eq!(page.title, "Sound");
    }

    #[test]
    fn search_matches_title_and_keywords() {
        let results = search_pages("wifi");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Wi-Fi & Network");
    }

    #[test]
    fn search_is_case_insensitive_and_matches_multiple_pages() {
        let results = search_pages("time");

        assert!(results.iter().any(|page| page.title == "Date, Time & Locale"));
        assert!(results.iter().all(|page| !page.title.is_empty()));
    }

    #[test]
    fn sound_is_the_first_real_page_kind() {
        let appearance = find_by_route_head("appearance").expect("appearance page should exist");
        let displays = find_by_route_head("displays").expect("displays page should exist");
        let power = find_by_route_head("power").expect("power page should exist");
        let sound = find_by_route_head("sound").expect("sound page should exist");
        let bluetooth = find_by_route_head("bluetooth").expect("bluetooth page should exist");
        let about = find_by_route_head("about").expect("about page should exist");

        assert_ne!(appearance.kind, PageKind::Stub);
        assert_eq!(bluetooth.kind, PageKind::Bluetooth);
        assert_eq!(displays.kind, PageKind::Displays);
        assert_ne!(power.kind, PageKind::Stub);
        assert_eq!(sound.kind, PageKind::Sound);
        assert_eq!(about.kind, PageKind::Stub);
    }

    #[test]
    fn sound_sections_cover_outputs_inputs_and_applications() {
        let sections = sound_sections();

        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].0, "Output");
        assert_eq!(sections[1].0, "Input");
        assert_eq!(sections[2].0, "Applications");
    }
}
