use relm4::gtk::{self, gdk, glib, prelude::*};

const DEFAULT_MIN_CONTENT_HEIGHT: i32 = 160;
const DEFAULT_CHROME_RESERVE: i32 = 144;

pub fn install_half_monitor_limit(
    popover: &gtk::Popover,
    scroller: &gtk::ScrolledWindow,
    parent: &gtk::Box,
) {
    apply_half_monitor_limit(scroller, parent, Some(popover));

    let parent = parent.clone();
    let scroller = scroller.clone();
    popover.connect_show(move |popover| {
        apply_half_monitor_limit(&scroller, &parent, Some(popover));

        let popover = popover.clone();
        let parent = parent.clone();
        let scroller = scroller.clone();
        glib::idle_add_local_once(move || {
            apply_half_monitor_limit(&scroller, &parent, Some(&popover));
        });
    });
}

fn apply_half_monitor_limit(
    scroller: &gtk::ScrolledWindow,
    parent: &gtk::Box,
    popover: Option<&gtk::Popover>,
) {
    let Some(height) = monitor_height(parent) else {
        return;
    };

    let chrome_height = popover
        .map(|popover| popover.height() - scroller.height())
        .filter(|height| *height > 0);
    scroller.set_max_content_height(content_height_limit(height, chrome_height));
}

fn content_height_limit(monitor_height: i32, chrome_height: Option<i32>) -> i32 {
    let chrome_height = chrome_height.unwrap_or(DEFAULT_CHROME_RESERVE);
    ((monitor_height / 2) - chrome_height).max(DEFAULT_MIN_CONTENT_HEIGHT)
}

fn monitor_height(parent: &gtk::Box) -> Option<i32> {
    if let Some(native) = parent.native() {
        if let Some(surface) = native.surface() {
            if let Some(monitor) = surface.display().monitor_at_surface(&surface) {
                return Some(monitor.geometry().height());
            }
        }
    }

    first_monitor_height()
}

fn first_monitor_height() -> Option<i32> {
    let display = gdk::Display::default()?;
    let monitor = display.monitors().item(0).and_downcast::<gdk::Monitor>()?;
    Some(monitor.geometry().height())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_height_limit_subtracts_chrome_from_half_monitor() {
        assert_eq!(content_height_limit(1200, Some(140)), 460);
    }

    #[test]
    fn content_height_limit_uses_reserve_before_first_allocation() {
        assert_eq!(
            content_height_limit(1200, None),
            600 - DEFAULT_CHROME_RESERVE
        );
    }

    #[test]
    fn content_height_limit_never_drops_below_minimum() {
        assert_eq!(
            content_height_limit(500, Some(180)),
            DEFAULT_MIN_CONTENT_HEIGHT
        );
    }
}
