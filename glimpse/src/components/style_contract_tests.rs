const BASE_CSS: &str = include_str!("../../../themes/base.css");
const ADWAITA_CSS: &str = include_str!("../../../themes/adwaita.css");
const ACCENT_CSS: &str = include_str!("../../../themes/accent.css");

#[test]
fn base_css_defines_shared_motion_state_contract() {
    for selector in [
        ".action-row.is-selected .action-row__button",
        ".action-row.is-checked .action-row__button",
        ".action-row__button:focus-visible",
        ".card-surface:hover",
        ".badge",
        ".status-dot",
    ] {
        assert!(
            BASE_CSS.contains(selector),
            "base.css should define motion/state selector `{selector}`",
        );
    }

    assert!(
        BASE_CSS.contains("@media (prefers-reduced-motion: reduce)"),
        "base.css should define reduced-motion overrides",
    );
}

#[test]
fn base_css_keeps_notification_popover_and_popup_geometry_contract() {
    for selector in [
        ".notifications-popover .notif-group-collapsed .notif-group-lead.card-surface",
        ".notification-popup",
        ".popup-overflow",
    ] {
        assert!(
            BASE_CSS.contains(selector),
            "base.css should define notification selector `{selector}`",
        );
    }

    for token in [
        "--notification-popover-min-width:",
        "--notification-popover-min-height:",
        "--notification-popup-min-width:",
        "--notification-card-padding:",
        "--notification-card-radius:",
        "--notification-popup-shadow:",
        "--notification-control-size:",
        "--notification-summary-size:",
        "--notification-body-size:",
        "--notification-time-size:",
    ] {
        assert!(
            BASE_CSS.contains(token),
            "base.css should define notification token `{token}`",
        );
    }
}

#[test]
fn notification_styles_rely_on_shared_hero_card_and_footer_primitives() {
    for selector in [
        ".notifications-popover .hero-row__title",
        ".notifications-popover .hero-row__subtitle",
        ".notifications-popover .card-surface {",
        ".notifications-popover .card-surface:hover",
        ".notifications-popover .card-surface__header",
        ".notifications-popover .footer-action .action-row__title",
        ".notification-popup .card-surface",
        ".notification-popup .card-surface:hover",
        ".notification-popup .card-surface__header",
    ] {
        assert!(
            !BASE_CSS.contains(selector),
            "base.css should not keep notification override `{selector}` once shared primitives cover it",
        );
    }
}

#[test]
fn adwaita_css_maps_semantic_tokens_from_gtk_symbolic_colors() {
    for token in [
        "--color-bg:",
        "--color-fg:",
        "--color-surface:",
        "--color-surface-raised:",
        "--color-border:",
        "--color-accent:",
        "--color-warning:",
        "--popover-bg:",
        "--card-bg:",
    ] {
        assert!(
            ADWAITA_CSS.contains(token),
            "adwaita.css should define `{token}`",
        );
    }

    for symbolic in [
        "@window_bg_color",
        "@window_fg_color",
        "@view_bg_color",
        "@view_fg_color",
        "@card_bg_color",
        "@accent_bg_color",
        "@warning_color",
    ] {
        assert!(
            ADWAITA_CSS.contains(symbolic),
            "adwaita.css should map from GTK symbolic color `{symbolic}`",
        );
    }
}

#[test]
fn accent_css_exposes_only_system_accent_contract() {
    assert!(
        ACCENT_CSS.contains("--sys-accent:"),
        "accent.css should define --sys-accent",
    );
    assert!(
        ACCENT_CSS.contains("--sys-accent-fg:"),
        "accent.css should define --sys-accent-fg",
    );
    assert!(
        !ACCENT_CSS.contains("--color-bg:"),
        "accent.css should not redefine full theme tokens",
    );
}
