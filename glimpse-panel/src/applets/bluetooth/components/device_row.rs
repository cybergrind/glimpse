use std::cell::Cell;
use std::rc::Rc;

use relm4::gtk::{self, prelude::*};

use super::{BluetoothCommand, BluetoothCommandSender, BluetoothDeviceAction, BtDevice};

pub struct DeviceRow {
    pub button: gtk::Button,
    icon: gtk::Image,
    name_label: gtk::Label,
    battery_label: gtk::Label,
    spinner: gtk::Spinner,
    pub popover_menu: gtk::PopoverMenu,
    connecting: Rc<Cell<bool>>,
    pending_action: Rc<Cell<Option<BluetoothDeviceAction>>>,
    connected: Rc<Cell<bool>>,
    paired: Rc<Cell<bool>>,
}

impl DeviceRow {
    pub fn new(dev: &BtDevice, on_command: BluetoothCommandSender) -> Self {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("bt-device-row");

        let icon = gtk::Image::from_icon_name(&dev.icon);
        icon.set_pixel_size(16);
        icon.set_valign(gtk::Align::Center);
        row.append(&icon);

        let name_label = gtk::Label::new(Some(&dev.name));
        name_label.set_hexpand(true);
        name_label.set_halign(gtk::Align::Start);
        name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name_label.set_max_width_chars(25);
        row.append(&name_label);

        let right_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        right_box.set_valign(gtk::Align::Center);

        let battery_label = gtk::Label::new(None);
        battery_label.add_css_class("bt-battery");
        battery_label.set_visible(false);
        right_box.append(&battery_label);

        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        spinner.set_size_request(16, 16);
        right_box.append(&spinner);

        row.append(&right_box);

        let btn = gtk::Button::new();
        btn.set_child(Some(&row));
        btn.add_css_class("flat");
        btn.add_css_class("bt-device-btn");

        let connecting = Rc::new(Cell::new(false));
        let pending_action = Rc::new(Cell::new(None));
        let connected = Rc::new(Cell::new(dev.connected));
        let paired = Rc::new(Cell::new(dev.paired));

        {
            let addr = dev.address.clone();
            let dev_name = dev.name.clone();
            let conn_flag = connecting.clone();
            let spin = spinner.clone();
            let pending = pending_action.clone();
            let conn_state = connected.clone();
            let pair_state = paired.clone();
            let on_command = on_command.clone();
            btn.connect_clicked(move |_| {
                if conn_flag.get() {
                    tracing::debug!(address = %addr, "bluetooth ui: ignoring click while action pending");
                    return;
                }
                let action = if conn_state.get() {
                    BluetoothDeviceAction::Disconnect
                } else if pair_state.get() {
                    BluetoothDeviceAction::Connect
                } else {
                    BluetoothDeviceAction::Pair
                };
                start_device_action(
                    &on_command,
                    action,
                    addr.clone(),
                    dev_name.clone(),
                    conn_flag.clone(),
                    pending.clone(),
                    spin.clone(),
                );
            });
        }

        let menu = build_menu(dev.connected, dev.paired);
        let popover_menu = gtk::PopoverMenu::from_model(Some(&menu));
        popover_menu.set_parent(&btn);
        popover_menu.set_has_arrow(false);

        let action_group = gtk::gio::SimpleActionGroup::new();
        setup_actions(
            &action_group,
            on_command,
            &dev.address,
            &dev.name,
            &connecting,
            &pending_action,
            &spinner,
        );
        btn.insert_action_group("bt", Some(&action_group));

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        let pm = popover_menu.clone();
        right_click.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            pm.popup();
        });
        btn.add_controller(right_click);

        apply_icon_style(&icon, dev.connected);
        apply_tooltip(&btn, dev);
        apply_battery(&battery_label, dev.battery);

        DeviceRow {
            button: btn,
            icon,
            name_label,
            battery_label,
            spinner,
            popover_menu,
            connecting,
            pending_action,
            connected,
            paired,
        }
    }

    pub fn update(&self, dev: &BtDevice) {
        self.connected.set(dev.connected);
        self.paired.set(dev.paired);
        self.icon.set_icon_name(Some(&dev.icon));
        apply_icon_style(&self.icon, dev.connected);
        self.name_label.set_label(&dev.name);
        apply_tooltip(&self.button, dev);
        apply_battery(&self.battery_label, dev.battery);

        let menu = build_menu(dev.connected, dev.paired);
        self.popover_menu.set_menu_model(Some(&menu));

        if self.connecting.get()
            && self
                .pending_action
                .get()
                .is_some_and(|action| action_observed_complete(action, dev))
        {
            self.finish_action();
        }
    }

    pub fn finish_action(&self) {
        self.connecting.set(false);
        self.pending_action.set(None);
        self.spinner.stop();
        self.spinner.set_visible(false);
    }
}

fn build_menu(connected: bool, paired: bool) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    if connected {
        menu.append(Some("Disconnect"), Some("bt.disconnect"));
    } else {
        menu.append(Some("Connect"), Some("bt.connect"));
    }
    if !paired {
        menu.append(Some("Pair"), Some("bt.pair"));
    }
    menu.append(Some("Forget"), Some("bt.forget"));
    menu
}

fn setup_actions(
    group: &gtk::gio::SimpleActionGroup,
    on_command: BluetoothCommandSender,
    address: &str,
    name: &str,
    connecting: &Rc<Cell<bool>>,
    pending_action: &Rc<Cell<Option<BluetoothDeviceAction>>>,
    spinner: &gtk::Spinner,
) {
    for (action_name, action_kind) in [
        ("disconnect", BluetoothDeviceAction::Disconnect),
        ("connect", BluetoothDeviceAction::Connect),
        ("pair", BluetoothDeviceAction::Pair),
    ] {
        let addr = address.to_owned();
        let dev_name = name.to_owned();
        let conn = connecting.clone();
        let pending = pending_action.clone();
        let spin = spinner.clone();
        let on_command = on_command.clone();
        let action = gtk::gio::SimpleAction::new(action_name, None);
        action.connect_activate(move |_, _| {
            if conn.get() {
                return;
            }
            start_device_action(
                &on_command,
                action_kind,
                addr.clone(),
                dev_name.clone(),
                conn.clone(),
                pending.clone(),
                spin.clone(),
            );
        });
        group.add_action(&action);
    }

    let addr = address.to_owned();
    let dev_name = name.to_owned();
    let conn = connecting.clone();
    let pending = pending_action.clone();
    let spin = spinner.clone();
    let on_command = on_command.clone();
    let action = gtk::gio::SimpleAction::new("forget", None);
    action.connect_activate(move |_, _| {
        if conn.get() {
            return;
        }
        start_device_action(
            &on_command,
            BluetoothDeviceAction::Forget,
            addr.clone(),
            dev_name.clone(),
            conn.clone(),
            pending.clone(),
            spin.clone(),
        );
    });
    group.add_action(&action);
}

fn start_device_action(
    on_command: &BluetoothCommandSender,
    action: BluetoothDeviceAction,
    address: String,
    dev_name: String,
    connecting: Rc<Cell<bool>>,
    pending_action: Rc<Cell<Option<BluetoothDeviceAction>>>,
    spinner: gtk::Spinner,
) {
    tracing::info!(
        ?action,
        address = %address,
        name = %dev_name,
        "bluetooth ui: device action clicked"
    );
    connecting.set(true);
    pending_action.set(Some(action));
    spinner.set_visible(true);
    spinner.start();
    on_command(BluetoothCommand::DeviceAction {
        address,
        name: dev_name,
        action,
    });
}

fn action_observed_complete(action: BluetoothDeviceAction, dev: &BtDevice) -> bool {
    match action {
        BluetoothDeviceAction::Connect => dev.connected,
        BluetoothDeviceAction::Disconnect => !dev.connected,
        BluetoothDeviceAction::Pair => dev.paired,
        BluetoothDeviceAction::Forget => !dev.paired && !dev.trusted,
    }
}

fn apply_icon_style(icon: &gtk::Image, connected: bool) {
    if connected {
        icon.remove_css_class("bt-device-icon");
        icon.add_css_class("bt-device-icon-active");
    } else {
        icon.remove_css_class("bt-device-icon-active");
        icon.add_css_class("bt-device-icon");
    }
}

fn apply_tooltip(btn: &gtk::Button, dev: &BtDevice) {
    let mut parts = Vec::new();
    if !dev.device_type.is_empty() && dev.device_type != "Device" {
        parts.push(dev.device_type.clone());
    }
    if let Some(pct) = dev.battery {
        parts.push(format!("{pct}%"));
    }
    if dev.connected {
        parts.push("Connected".into());
    } else if dev.paired {
        parts.push("Paired".into());
    }
    let tooltip = if parts.is_empty() {
        dev.name.clone()
    } else {
        parts.join(" \u{b7} ")
    };
    btn.set_tooltip_text(Some(&tooltip));
}

fn apply_battery(label: &gtk::Label, battery: Option<u8>) {
    if let Some(pct) = battery {
        label.set_label(&format!("{pct}%"));
        label.set_visible(true);
    } else {
        label.set_visible(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(connected: bool, paired: bool, trusted: bool) -> BtDevice {
        BtDevice {
            address: "AA:BB:CC:DD:EE:FF".into(),
            name: "Device".into(),
            icon: "bluetooth-symbolic".into(),
            device_type: "Device".into(),
            paired,
            trusted,
            connected,
            battery: None,
            rssi: None,
        }
    }

    #[test]
    fn observed_completion_matches_device_state() {
        assert!(action_observed_complete(
            BluetoothDeviceAction::Connect,
            &device(true, true, true)
        ));
        assert!(action_observed_complete(
            BluetoothDeviceAction::Disconnect,
            &device(false, true, true)
        ));
        assert!(action_observed_complete(
            BluetoothDeviceAction::Pair,
            &device(false, true, true)
        ));
        assert!(!action_observed_complete(
            BluetoothDeviceAction::Forget,
            &device(false, false, true)
        ));
    }
}
