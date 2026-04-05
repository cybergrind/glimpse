use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use glimpse_client::Client;
use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BtDevice {
    pub address: String,
    pub name: String,
    pub icon: String,
    pub device_type: String,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    pub battery: Option<u8>,
    pub rssi: Option<i16>,
}

struct DeviceRow {
    button: gtk::Button,
    icon: gtk::Image,
    name_label: gtk::Label,
    battery_label: gtk::Label,
    spinner: gtk::Spinner,
    popover_menu: gtk::PopoverMenu,
    connecting: Rc<Cell<bool>>,
    connected: Rc<Cell<bool>>,
    paired: Rc<Cell<bool>>,
}

pub struct BluetoothPopover {
    popover: gtk::Popover,
    client: Arc<Client>,
    hero_icon: gtk::Image,
    subtitle: gtk::Label,
    power_switch: gtk::Switch,
    device_box: gtk::Box,
    empty_label: gtk::Label,
    scan_btn: gtk::Button,
    scan_label: gtk::Label,
    rows: HashMap<String, DeviceRow>,
    updating_power: Rc<Cell<bool>>,
    powered: bool,
    connected_count: u32,
}

pub struct BluetoothPopoverInit {
    pub parent: gtk::Box,
    pub client: Arc<Client>,
    pub settings_command: String,
}

#[derive(Debug)]
pub enum BluetoothPopoverInput {
    Toggle,
    UpdateStatus { powered: bool, discovering: bool },
    UpdateDevices(Vec<BtDevice>),
    ScanStarted,
}

fn spawn_call(client: &Arc<Client>, method: &'static str, params: serde_json::Value) {
    let c = client.clone();
    glib::spawn_future_local(async move { let _ = c.call(method, params).await; });
}

fn spawn_call_with_notify(
    client: &Arc<Client>, method: &'static str, params: serde_json::Value,
    device_name: String, connecting: Rc<Cell<bool>>, spinner: gtk::Spinner,
) {
    let c = client.clone();
    connecting.set(true);
    spinner.set_visible(true);
    spinner.start();
    glib::spawn_future_local(async move {
        let result = c.call(method, params).await;
        connecting.set(false);
        spinner.stop();
        spinner.set_visible(false);
        if let Err(e) = result {
            let action = match method {
                "bluetooth.connect" => "connect to",
                "bluetooth.disconnect" => "disconnect from",
                "bluetooth.pair" => "pair with",
                _ => "operate on",
            };
            let msg = format!("Failed to {} {}: {}", action, device_name, e);
            tracing::warn!("{}", msg);
            let _ = std::process::Command::new("notify-send")
                .args(["Bluetooth", &msg])
                .spawn();
        }
    });
}

fn looks_like_mac(s: &str) -> bool {
    let s = s.trim();
    if s.len() < 11 { return false; }
    let sep = if s.contains(':') { ':' } else if s.contains('-') { '-' } else { return false };
    let parts: Vec<&str> = s.split(sep).collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

fn is_visible_device(dev: &BtDevice) -> bool {
    if dev.name.is_empty() || looks_like_mac(&dev.name) {
        return dev.connected || dev.paired || dev.trusted;
    }
    dev.connected || dev.paired || dev.trusted || dev.rssi.is_some()
}

impl SimpleComponent for BluetoothPopover {
    type Init = BluetoothPopoverInit;
    type Input = BluetoothPopoverInput;
    type Output = ();
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root { gtk::Popover::new() }

    fn init(
        init: Self::Init, root: Self::Root, sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("bluetooth-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        // === Hero ===
        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero.add_css_class("bt-hero");

        let hero_icon = gtk::Image::from_icon_name("bluetooth-active-symbolic");
        hero_icon.set_pixel_size(32);
        hero.append(&hero_icon);

        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        title_box.set_hexpand(true);
        title_box.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some("Bluetooth"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("bt-title");
        title_box.append(&title);
        let subtitle = gtk::Label::new(Some("Off"));
        subtitle.set_halign(gtk::Align::Start);
        subtitle.add_css_class("bt-subtitle");
        title_box.append(&subtitle);
        hero.append(&title_box);

        let power_switch = gtk::Switch::new();
        power_switch.set_valign(gtk::Align::Center);
        power_switch.set_tooltip_text(Some("Toggle all adapters"));
        let updating_power = Rc::new(Cell::new(false));
        let guard = updating_power.clone();
        let c = init.client.clone();
        power_switch.connect_state_set(move |_, active| {
            if guard.get() { return glib::Propagation::Stop; }
            spawn_call(&c, "bluetooth.set_powered", serde_json::json!({"powered": active}));
            glib::Propagation::Stop
        });
        hero.append(&power_switch);

        vbox.append(&hero);
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Device list ===
        let empty_label = gtk::Label::new(Some("No devices"));
        empty_label.set_halign(gtk::Align::Start);
        empty_label.add_css_class("bt-empty");
        vbox.append(&empty_label);

        let device_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        device_box.add_css_class("bt-device-list");

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_max_content_height(300);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&device_box));
        vbox.append(&scroll);

        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Scan button ===
        let scan_lbl = gtk::Label::new(Some("Scan for devices"));
        scan_lbl.set_halign(gtk::Align::Start);
        let scan_btn = gtk::Button::new();
        scan_btn.set_child(Some(&scan_lbl));
        scan_btn.add_css_class("flat");
        scan_btn.add_css_class("settings-btn");
        let s = sender.clone();
        scan_btn.connect_clicked(move |_| {
            s.input(BluetoothPopoverInput::ScanStarted);
        });
        vbox.append(&scan_btn);

        // === Settings ===
        if !init.settings_command.is_empty() {
            let cmd = init.settings_command;
            let lbl = gtk::Label::new(Some("Bluetooth Settings"));
            lbl.set_halign(gtk::Align::Start);
            let btn = gtk::Button::new();
            btn.set_child(Some(&lbl));
            btn.add_css_class("flat");
            btn.add_css_class("settings-btn");
            btn.connect_clicked(move |_| {
                let parts: Vec<&str> = cmd.split_whitespace().collect();
                if let Some((&prog, args)) = parts.split_first() {
                    let _ = std::process::Command::new(prog).args(args).spawn();
                }
            });
            vbox.append(&btn);
        }

        root.set_child(Some(&vbox));

        // Auto-scan on popover open, stop on close.
        let s = sender.clone();
        root.connect_show(move |_| {
            s.input(BluetoothPopoverInput::ScanStarted);
        });
        let c = init.client.clone();
        let btn_ref = scan_btn.clone();
        let lbl_ref = scan_lbl.clone();
        root.connect_closed(move |_| {
            if !btn_ref.is_sensitive() {
                spawn_call(&c, "bluetooth.stop_discovery", serde_json::json!({}));
                btn_ref.set_sensitive(true);
                lbl_ref.set_label("Scan for devices");
            }
        });

        let model = BluetoothPopover {
            popover: root.clone(), client: init.client,
            hero_icon, subtitle, power_switch,
            device_box, empty_label,
            scan_btn, scan_label: scan_lbl,
            rows: HashMap::new(),
            updating_power, powered: false, connected_count: 0,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BluetoothPopoverInput::Toggle => {
                if self.popover.is_visible() { self.popover.popdown(); }
                else { self.popover.popup(); }
            }
            BluetoothPopoverInput::UpdateStatus { powered, discovering } => {
                self.powered = powered;

                // If daemon says discovery stopped, reset scan button.
                if !discovering && !self.scan_btn.is_sensitive() {
                    self.scan_btn.set_sensitive(true);
                    self.scan_label.set_label("Scan for devices");
                }

                if self.power_switch.is_active() != powered {
                    self.updating_power.set(true);
                    self.power_switch.set_active(powered);
                    self.power_switch.set_state(powered);
                    self.updating_power.set(false);
                }

                self.hero_icon.set_icon_name(Some(if powered {
                    "bluetooth-active-symbolic"
                } else {
                    "bluetooth-disabled-symbolic"
                }));

                self.update_subtitle();
            }
            BluetoothPopoverInput::UpdateDevices(devices) => {
                let mut visible: Vec<&BtDevice> = devices.iter()
                    .filter(|d| is_visible_device(d))
                    .collect();
                visible.sort_by(|a, b| {
                    b.connected.cmp(&a.connected)
                        .then(b.paired.cmp(&a.paired))
                        .then(b.rssi.unwrap_or(i16::MIN).cmp(&a.rssi.unwrap_or(i16::MIN)))
                });

                self.connected_count = visible.iter().filter(|d| d.connected).count() as u32;

                // Remove rows for devices no longer visible.
                let visible_addrs: std::collections::HashSet<&str> =
                    visible.iter().map(|d| d.address.as_str()).collect();
                let to_remove: Vec<String> = self.rows.keys()
                    .filter(|addr| !visible_addrs.contains(addr.as_str()))
                    .cloned()
                    .collect();
                for addr in to_remove {
                    if let Some(row) = self.rows.remove(&addr) {
                        row.popover_menu.unparent();
                        self.device_box.remove(&row.button);
                    }
                }

                // Update existing rows or create new ones.
                for (i, dev) in visible.iter().enumerate() {
                    if let Some(row) = self.rows.get(&dev.address) {
                        update_row(row, dev);
                        // Reorder: move to correct position.
                        if i == 0 {
                            self.device_box.reorder_child_after(&row.button, Option::<&gtk::Widget>::None);
                        } else if let Some(prev) = visible.get(i - 1) {
                            if let Some(prev_row) = self.rows.get(&prev.address) {
                                self.device_box.reorder_child_after(&row.button, Some(&prev_row.button));
                            }
                        }
                    } else {
                        let row = create_row(dev, &self.client);
                        self.device_box.append(&row.button);
                        self.rows.insert(dev.address.clone(), row);
                    }
                }

                self.empty_label.set_visible(visible.is_empty());
                self.update_subtitle();
            }
            BluetoothPopoverInput::ScanStarted => {
                self.scan_btn.set_sensitive(false);
                self.scan_label.set_label("Scanning…");
                spawn_call(&self.client, "bluetooth.start_discovery", serde_json::json!({}));
                let client = self.client.clone();
                let btn = self.scan_btn.clone();
                let label = self.scan_label.clone();
                glib::timeout_add_local_once(std::time::Duration::from_secs(10), move || {
                    if !btn.is_sensitive() {
                        spawn_call(&client, "bluetooth.stop_discovery", serde_json::json!({}));
                        btn.set_sensitive(true);
                        label.set_label("Scan for devices");
                    }
                });
                self.update_subtitle();
            }
        }
    }
}

impl BluetoothPopover {
    fn update_subtitle(&self) {
        let text = if !self.powered {
            "Off".into()
        } else if self.connected_count > 0 {
            format!("On · {} connected", self.connected_count)
        } else {
            "On".into()
        };
        self.subtitle.set_label(&text);
    }
}

fn create_row(dev: &BtDevice, client: &Arc<Client>) -> DeviceRow {
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
    let connected = Rc::new(Cell::new(dev.connected));
    let paired = Rc::new(Cell::new(dev.paired));

    // Left click.
    let c = client.clone();
    let addr = dev.address.clone();
    let dev_name = dev.name.clone();
    let conn_flag = connecting.clone();
    let spin = spinner.clone();
    let conn_state = connected.clone();
    let pair_state = paired.clone();
    btn.connect_clicked(move |_| {
        if conn_flag.get() { return; }
        let method = if conn_state.get() {
            "bluetooth.disconnect"
        } else if pair_state.get() {
            "bluetooth.connect"
        } else {
            "bluetooth.pair"
        };
        spawn_call_with_notify(
            &c, method, serde_json::json!({"address": addr}),
            dev_name.clone(), conn_flag.clone(), spin.clone(),
        );
    });

    // Right click context menu.
    let menu = gtk::gio::Menu::new();
    if dev.connected {
        menu.append(Some("Disconnect"), Some("bt.disconnect"));
    } else {
        menu.append(Some("Connect"), Some("bt.connect"));
    }
    if !dev.paired {
        menu.append(Some("Pair"), Some("bt.pair"));
    }
    menu.append(Some("Forget"), Some("bt.forget"));

    let popover_menu = gtk::PopoverMenu::from_model(Some(&menu));
    popover_menu.set_parent(&btn);
    popover_menu.set_has_arrow(false);

    let action_group = gtk::gio::SimpleActionGroup::new();
    setup_actions(&action_group, client, &dev.address, &dev.name, &connecting, &spinner);
    btn.insert_action_group("bt", Some(&action_group));

    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    let pm = popover_menu.clone();
    right_click.connect_pressed(move |gesture, _, _, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        pm.popup();
    });
    btn.add_controller(right_click);

    // Apply initial state.
    apply_icon_style(&icon, dev.connected);
    apply_tooltip(&btn, dev);
    apply_battery(&battery_label, dev.battery);

    DeviceRow {
        button: btn, icon, name_label, battery_label,
        spinner, popover_menu, connecting, connected, paired,
    }
}

fn update_row(row: &DeviceRow, dev: &BtDevice) {
    // Don't update while connecting — spinner is showing, state is in flux.
    if row.connecting.get() { return; }

    row.connected.set(dev.connected);
    row.paired.set(dev.paired);
    row.icon.set_icon_name(Some(&dev.icon));
    apply_icon_style(&row.icon, dev.connected);
    row.name_label.set_label(&dev.name);
    apply_tooltip(&row.button, dev);
    apply_battery(&row.battery_label, dev.battery);

    // Rebuild context menu.
    let menu = gtk::gio::Menu::new();
    if dev.connected {
        menu.append(Some("Disconnect"), Some("bt.disconnect"));
    } else {
        menu.append(Some("Connect"), Some("bt.connect"));
    }
    if !dev.paired {
        menu.append(Some("Pair"), Some("bt.pair"));
    }
    menu.append(Some("Forget"), Some("bt.forget"));
    row.popover_menu.set_menu_model(Some(&menu));
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
    let tooltip = if dev.connected { "Disconnect" }
        else if dev.paired { "Connect" }
        else { "Pair" };
    btn.set_tooltip_text(Some(tooltip));
}

fn apply_battery(label: &gtk::Label, battery: Option<u8>) {
    if let Some(pct) = battery {
        label.set_label(&format!("{pct}%"));
        label.set_visible(true);
    } else {
        label.set_visible(false);
    }
}

fn setup_actions(
    group: &gtk::gio::SimpleActionGroup,
    client: &Arc<Client>, address: &str, name: &str,
    connecting: &Rc<Cell<bool>>, spinner: &gtk::Spinner,
) {
    for (action_name, method) in [
        ("disconnect", "bluetooth.disconnect"),
        ("connect", "bluetooth.connect"),
        ("pair", "bluetooth.pair"),
    ] {
        let c = client.clone();
        let addr = address.to_owned();
        let dev_name = name.to_owned();
        let conn = connecting.clone();
        let spin = spinner.clone();
        let action = gtk::gio::SimpleAction::new(action_name, None);
        action.connect_activate(move |_, _| {
            if conn.get() { return; }
            spawn_call_with_notify(
                &c, method, serde_json::json!({"address": addr}),
                dev_name.clone(), conn.clone(), spin.clone(),
            );
        });
        group.add_action(&action);
    }

    let c = client.clone();
    let addr = address.to_owned();
    let action = gtk::gio::SimpleAction::new("forget", None);
    action.connect_activate(move |_, _| {
        spawn_call(&c, "bluetooth.forget", serde_json::json!({"address": addr}));
    });
    group.add_action(&action);
}
