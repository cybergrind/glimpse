use relm4::{
    ComponentSender,
    gtk::{self, prelude::*},
};

use crate::applets::exec::{
    applet::{Exec, ExecMsg},
    protocol::{CallbackData, PanelMessage, StatusItem},
    renderer::apply_icon_to_image,
};

pub fn build_status_item(
    item: &StatusItem,
    index: usize,
    has_popover: bool,
    sender: &ComponentSender<Exec>,
) -> gtk::Box {
    let fallback_item = item.clone();
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    container.add_css_class("exec-status-item");

    if let Some(icon) = &item.icon {
        let image = gtk::Image::new();
        apply_icon_to_image(&image, icon);
        image.set_pixel_size(16);
        container.append(&image);
    }
    if let Some(text) = &item.text {
        let label = gtk::Label::new(Some(text));
        label.add_css_class("exec-status-label");
        container.append(&label);
    }

    let click_sender = sender.clone();
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        let button_name = mouse_button_name(gesture.current_button());
        let opens_popover = has_popover && gesture.current_button() == 1;
        if opens_popover {
            click_sender.input(ExecMsg::TogglePopover);
        }
        if let Some(callback) =
            status_click_callback(&fallback_item, index, button_name, opens_popover)
        {
            if let PanelMessage::Callback(callback) = callback {
                click_sender.input(ExecMsg::Callback(callback));
            }
        }
    });
    container.add_controller(click);

    let scroll_id = item.id.clone();
    let scroll_sender = sender.clone();
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
    );
    scroll.connect_scroll(move |_, _, delta_y| {
        if let Some(id) = &scroll_id {
            scroll_sender.input(ExecMsg::Callback(CallbackData {
                id: id.clone(),
                event: "scroll".into(),
                delta_y: Some(delta_y),
                ..CallbackData::default()
            }));
        }
        gtk::glib::Propagation::Proceed
    });
    container.add_controller(scroll);

    container
}

pub fn mouse_button_name(button: u32) -> &'static str {
    match button {
        1 => "left",
        2 => "middle",
        3 => "right",
        _ => "other",
    }
}

pub fn status_click_callback(
    item: &StatusItem,
    index: usize,
    button: &str,
    opens_popover: bool,
) -> Option<PanelMessage> {
    if opens_popover && button == "left" {
        return None;
    }
    PanelMessage::status_click(item, index, button)
}

#[cfg(test)]
mod tests {
    use super::{mouse_button_name, status_click_callback};
    use crate::applets::exec::protocol::{CallbackData, PanelMessage, StatusItem};

    #[test]
    fn status_item_callbacks_prefer_ids() {
        let item = StatusItem {
            id: Some("wifi".into()),
            icon: None,
            text: Some("Online".into()),
        };

        let callback = PanelMessage::status_click(&item, 0, "left");

        assert_eq!(
            callback,
            Some(PanelMessage::Callback(CallbackData {
                id: "wifi".into(),
                event: "click".into(),
                button: Some("left".into()),
                ..CallbackData::default()
            }))
        );
    }

    #[test]
    fn left_click_that_opens_popover_does_not_also_emit_status_callback() {
        let item = StatusItem {
            id: Some("deploy_status".into()),
            icon: None,
            text: Some("Ready".into()),
        };

        let callback = status_click_callback(&item, 0, "left", true);

        assert_eq!(callback, None);
    }

    #[test]
    fn mouse_button_names_match_protocol_values() {
        assert_eq!(mouse_button_name(1), "left");
        assert_eq!(mouse_button_name(2), "middle");
        assert_eq!(mouse_button_name(3), "right");
        assert_eq!(mouse_button_name(9), "other");
    }
}
