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
#[allow(dead_code)]
pub struct WifiAp {
    pub ssid: String,
    pub strength: u8,
    pub frequency: u32,
    pub security: String,
    pub connected: bool,
    pub saved: bool,
    pub uuid: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NetDevice {
    pub interface: String,
    pub device_type: String,
    pub state: String,
    pub speed: u32,
    pub carrier: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[allow(dead_code)]
pub struct VpnEntry {
    pub id: String,
    pub uuid: String,
    pub connection_type: String,
    pub active: bool,
    pub state: Option<String>,
}

#[allow(dead_code)]
struct ApRow {
    button: gtk::Button,
    icon: gtk::Image,
    name_label: gtk::Label,
    lock_icon: gtk::Image,
    spinner: gtk::Spinner,
    popover_menu: Option<gtk::PopoverMenu>,
    connecting: Rc<Cell<bool>>,
    connected: Rc<Cell<bool>>,
}

#[allow(dead_code)]
struct VpnRow {
    button: gtk::Button,
    spinner: gtk::Spinner,
    connecting: Rc<Cell<bool>>,
    active: Rc<Cell<bool>>,
}

#[allow(dead_code)]
pub struct NetworkPopover {
    popover: gtk::Popover,
    client: Arc<Client>,
    hero_icon: gtk::Image,
    subtitle: gtk::Label,
    wifi_switch: gtk::Switch,
    wifi_section: gtk::Box,
    ap_box: gtk::Box,
    wifi_empty_label: gtk::Label,
    scan_interval: u64,
    eth_section: gtk::Box,
    eth_device_box: gtk::Box,
    vpn_section: gtk::Box,
    vpn_box: gtk::Box,
    ap_rows: HashMap<String, ApRow>,
    vpn_rows: HashMap<String, VpnRow>,
    updating_wifi_switch: Rc<Cell<bool>>,
    wifi_enabled: bool,
    primary_connection: String,
    primary_type: String,
    metered: bool,
}

pub struct NetworkPopoverInit {
    pub parent: gtk::Box,
    pub client: Arc<Client>,
    pub settings_command: String,
    pub scan_interval: u64,
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum NetworkPopoverInput {
    Toggle,
    UpdateStatus {
        primary_connection: String,
        primary_type: String,
        speed: u32,
        metered: bool,
        wifi_enabled: bool,
        connectivity: String,
        icon: String,
    },
    UpdateConnections(serde_json::Value),
    UpdateWifi(serde_json::Value),
    UpdateDevices(serde_json::Value),
    UpdateSavedVpns(serde_json::Value),
    ScanStarted,
}

fn spawn_call(client: &Arc<Client>, method: &'static str, params: serde_json::Value) {
    let c = client.clone();
    glib::spawn_future_local(async move { let _ = c.call(method, params).await; });
}


fn spawn_password_dialog(client: &Arc<Client>, ssid: String, connecting: Rc<Cell<bool>>, spinner: gtk::Spinner) {
    tracing::info!("network: requesting password for \"{}\"", ssid);
    let c = client.clone();
    connecting.set(true);
    spinner.set_visible(true);
    spinner.start();
    glib::spawn_future_local(async move {
        let result = tokio::task::spawn_blocking({
            let ssid = ssid.clone();
            move || {
                std::process::Command::new("zenity")
                    .args([
                        "--password",
                        "--title=WiFi Password",
                        &format!("--text=Enter password for {}", ssid),
                    ])
                    .output()
            }
        }).await;

        let password = result.ok()
            .and_then(|r| r.ok())
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(password) = password {
            tracing::info!("network: connecting to \"{}\" with password", ssid);
            let result = c.call("network.connect", serde_json::json!({
                "ssid": ssid,
                "password": password,
            })).await;
            match &result {
                Ok(_) => tracing::info!("network: connect to \"{}\" succeeded", ssid),
                Err(e) => {
                    tracing::warn!("network: connect to \"{}\" failed: {}", ssid, e);
                    let _ = std::process::Command::new("notify-send")
                        .args(["Network", &format!("Failed to connect to {}: {}", ssid, e)])
                        .spawn();
                }
            }
        } else {
            tracing::info!("network: password dialog cancelled for \"{}\"", ssid);
        }

        connecting.set(false);
        spinner.stop();
        spinner.set_visible(false);
    });
}

/// Like spawn_call_with_spinner, but keeps the spinner running after the call
/// succeeds. The spinner is cleared by update_ap_row when the state changes,
/// or by a timeout.
fn spawn_connect_call(
    client: &Arc<Client>, method: &'static str, params: serde_json::Value,
    name: String, connecting: Rc<Cell<bool>>, spinner: gtk::Spinner,
) {
    tracing::info!("network: calling {} for \"{}\"", method, name);
    let c = client.clone();
    connecting.set(true);
    spinner.set_visible(true);
    spinner.start();

    // Timeout: clear spinner after 15 seconds if still connecting
    let conn_timeout = connecting.clone();
    let spin_timeout = spinner.clone();
    glib::timeout_add_local_once(std::time::Duration::from_secs(15), move || {
        if conn_timeout.get() {
            tracing::warn!("network: connection attempt timed out for \"{}\"", name);
            conn_timeout.set(false);
            spin_timeout.stop();
            spin_timeout.set_visible(false);
        }
    });

    glib::spawn_future_local(async move {
        let result = c.call(method, params).await;
        if let Err(e) = result {
            tracing::warn!("network: {} failed: {}", method, e);
            connecting.set(false);
            spinner.stop();
            spinner.set_visible(false);
            let _ = std::process::Command::new("notify-send")
                .args(["Network", &format!("Connection failed: {}", e)])
                .spawn();
        }
        // On success: keep spinner running — update_ap_row will clear it
    });
}

fn band_label(frequency: u32) -> &'static str {
    if frequency < 3000 { "2.4 GHz" }
    else if frequency < 6000 { "5 GHz" }
    else { "6 GHz" }
}

fn ap_tooltip(ap: &WifiAp) -> String {
    let mut parts = Vec::new();
    if ap.security != "open" {
        parts.push(ap.security.to_uppercase());
    }
    if ap.frequency > 0 {
        parts.push(band_label(ap.frequency).to_string());
    }
    parts.push(format!("Signal: {}%", ap.strength));
    parts.join(" \u{b7} ")
}

fn signal_icon_name(strength: u8) -> &'static str {
    if strength >= 80 {
        "network-wireless-signal-excellent-symbolic"
    } else if strength >= 55 {
        "network-wireless-signal-good-symbolic"
    } else if strength >= 30 {
        "network-wireless-signal-ok-symbolic"
    } else if strength > 0 {
        "network-wireless-signal-weak-symbolic"
    } else {
        "network-wireless-signal-none-symbolic"
    }
}

impl SimpleComponent for NetworkPopover {
    type Init = NetworkPopoverInit;
    type Input = NetworkPopoverInput;
    type Output = ();
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root { gtk::Popover::new() }

    fn init(
        init: Self::Init, root: Self::Root, sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("network-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        // === Hero ===
        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero.add_css_class("net-hero");

        let hero_icon = gtk::Image::from_icon_name("network-offline-symbolic");
        hero_icon.set_pixel_size(32);
        hero.append(&hero_icon);

        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        title_box.set_hexpand(true);
        title_box.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some("Network"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("net-title");
        title_box.append(&title);
        let subtitle = gtk::Label::new(Some("Offline"));
        subtitle.set_halign(gtk::Align::Start);
        subtitle.add_css_class("net-subtitle");
        title_box.append(&subtitle);
        hero.append(&title_box);

        let wifi_switch = gtk::Switch::new();
        wifi_switch.set_valign(gtk::Align::Center);
        wifi_switch.set_tooltip_text(Some("Toggle WiFi"));
        let updating_wifi_switch = Rc::new(Cell::new(false));
        let guard = updating_wifi_switch.clone();
        let c = init.client.clone();
        wifi_switch.connect_state_set(move |_, active| {
            if guard.get() { return glib::Propagation::Stop; }
            spawn_call(&c, "network.set_wifi_enabled", serde_json::json!({"enabled": active}));
            glib::Propagation::Stop
        });
        hero.append(&wifi_switch);

        vbox.append(&hero);
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === WiFi section ===
        let wifi_section = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let wifi_empty_label = gtk::Label::new(Some("No access points"));
        wifi_empty_label.set_halign(gtk::Align::Start);
        wifi_empty_label.add_css_class("net-empty");
        wifi_section.append(&wifi_empty_label);

        let ap_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_max_content_height(300);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&ap_box));
        wifi_section.append(&scroll);

        vbox.append(&wifi_section);

        // === Ethernet section (hidden when no ethernet devices) ===
        let eth_section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        eth_section.set_visible(false);

        eth_section.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let eth_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        eth_header.add_css_class("net-section-header");
        let eth_title = gtk::Label::new(Some("Wired"));
        eth_title.set_halign(gtk::Align::Start);
        eth_title.set_hexpand(true);
        eth_title.add_css_class("net-section-title");
        eth_header.append(&eth_title);
        eth_section.append(&eth_header);

        let eth_device_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        eth_section.append(&eth_device_box);

        vbox.append(&eth_section);

        // === VPN section (hidden when no VPN connections) ===
        let vpn_section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vpn_section.set_visible(false);

        vpn_section.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let vpn_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        vpn_header.add_css_class("net-section-header");
        let vpn_title = gtk::Label::new(Some("VPN"));
        vpn_title.set_halign(gtk::Align::Start);
        vpn_title.set_hexpand(true);
        vpn_title.add_css_class("net-section-title");
        vpn_header.append(&vpn_title);
        vpn_section.append(&vpn_header);

        let vpn_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vpn_section.append(&vpn_box);

        vbox.append(&vpn_section);

        // === Settings button ===
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        if !init.settings_command.is_empty() {
            let cmd = init.settings_command;
            let lbl = gtk::Label::new(Some("Network Settings"));
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

        // Auto-scan: trigger immediately on open, repeat every scan_interval while open
        let scan_interval = init.scan_interval;
        let c = init.client.clone();
        let popover_ref = root.clone();
        root.connect_show(move |_| {
            spawn_call(&c, "network.wifi_scan", serde_json::json!({}));
            let c2 = c.clone();
            let p = popover_ref.clone();
            let interval = std::time::Duration::from_secs(scan_interval);
            glib::timeout_add_local(interval, move || {
                if p.is_visible() {
                    spawn_call(&c2, "network.wifi_scan", serde_json::json!({}));
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            });
        });

        let model = NetworkPopover {
            popover: root.clone(), client: init.client,
            hero_icon, subtitle, wifi_switch, wifi_section,
            ap_box, wifi_empty_label,
            scan_interval: init.scan_interval,
            eth_section, eth_device_box,
            vpn_section, vpn_box,
            ap_rows: HashMap::new(),
            vpn_rows: HashMap::new(),
            updating_wifi_switch,
            wifi_enabled: false,
            primary_connection: String::new(),
            primary_type: String::new(),
            metered: false,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NetworkPopoverInput::Toggle => {
                if self.popover.is_visible() { self.popover.popdown(); }
                else { self.popover.popup(); }
            }
            NetworkPopoverInput::UpdateStatus { primary_connection, primary_type, speed: _, metered, wifi_enabled, connectivity: _, icon } => {
                self.primary_connection = primary_connection;
                self.primary_type = primary_type;
                self.metered = metered;
                self.wifi_enabled = wifi_enabled;

                self.hero_icon.set_icon_name(Some(&icon));

                if self.wifi_switch.is_active() != wifi_enabled {
                    self.updating_wifi_switch.set(true);
                    self.wifi_switch.set_active(wifi_enabled);
                    self.wifi_switch.set_state(wifi_enabled);
                    self.updating_wifi_switch.set(false);
                }

                self.update_subtitle();
            }
            NetworkPopoverInput::UpdateConnections(_data) => {
                // Connections data forwarded from applet; currently handled via status
            }
            NetworkPopoverInput::UpdateWifi(data) => {
                let aps: Vec<WifiAp> = serde_json::from_value(data).unwrap_or_default();
                tracing::info!(count = aps.len(), "network popover: wifi update");
                self.update_ap_list(aps);
            }
            NetworkPopoverInput::UpdateDevices(data) => {
                let devices: Vec<NetDevice> = serde_json::from_value(data).unwrap_or_default();
                self.update_ethernet(devices);
            }
            NetworkPopoverInput::UpdateSavedVpns(data) => {
                let vpns: Vec<VpnEntry> = serde_json::from_value(data).unwrap_or_default();
                self.update_vpn_list(vpns);
            }
            NetworkPopoverInput::ScanStarted => {
                spawn_call(&self.client, "network.wifi_scan", serde_json::json!({}));
            }
        }
    }
}

impl NetworkPopover {
    fn update_subtitle(&self) {
        let text = if self.primary_connection.is_empty() {
            "Offline".into()
        } else {
            let mut parts = vec![self.primary_connection.clone()];
            if self.metered { parts.push("Metered".into()); }
            parts.join(" \u{b7} ")
        };
        self.subtitle.set_label(&text);
    }

    fn update_ap_list(&mut self, mut aps: Vec<WifiAp>) {
        aps.sort_by(|a, b| {
            b.connected.cmp(&a.connected)
                .then(b.saved.cmp(&a.saved))
                .then(b.strength.cmp(&a.strength))
        });

        let visible_ssids: std::collections::HashSet<&str> =
            aps.iter().map(|a| a.ssid.as_str()).collect();
        let to_remove: Vec<String> = self.ap_rows.keys()
            .filter(|ssid| !visible_ssids.contains(ssid.as_str()))
            .cloned()
            .collect();
        for ssid in to_remove {
            if let Some(row) = self.ap_rows.remove(&ssid) {
                if let Some(pm) = &row.popover_menu { pm.unparent(); }
                self.ap_box.remove(&row.button);
            }
        }

        for (i, ap) in aps.iter().enumerate() {
            if ap.ssid.is_empty() { continue; }
            if let Some(row) = self.ap_rows.get(&ap.ssid) {
                update_ap_row(row, ap);
                if i == 0 {
                    self.ap_box.reorder_child_after(&row.button, Option::<&gtk::Widget>::None);
                } else if let Some(prev) = aps.get(i - 1) {
                    if let Some(prev_row) = self.ap_rows.get(&prev.ssid) {
                        self.ap_box.reorder_child_after(&row.button, Some(&prev_row.button));
                    }
                }
            } else {
                let row = create_ap_row(ap, &self.client);
                self.ap_box.append(&row.button);
                self.ap_rows.insert(ap.ssid.clone(), row);
            }
        }

        self.wifi_empty_label.set_visible(aps.is_empty() || aps.iter().all(|a| a.ssid.is_empty()));
    }

    fn update_ethernet(&mut self, devices: Vec<NetDevice>) {
        let eth_devices: Vec<&NetDevice> = devices.iter()
            .filter(|d| d.device_type == "ethernet")
            .collect();

        // Clear and rebuild (simple, usually 1-2 devices)
        let mut child = self.eth_device_box.first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            self.eth_device_box.remove(&w);
        }

        self.eth_section.set_visible(!eth_devices.is_empty());

        for dev in &eth_devices {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

            let icon = gtk::Image::from_icon_name("network-wired-symbolic");
            icon.set_pixel_size(16);
            icon.set_valign(gtk::Align::Center);
            icon.add_css_class("net-ap-icon");
            row.append(&icon);

            let name_label = gtk::Label::new(Some(&dev.interface));
            name_label.set_hexpand(true);
            name_label.set_halign(gtk::Align::Start);
            row.append(&name_label);

            let info = if dev.state == "connected" {
                if dev.speed > 0 {
                    format!("{} Mbps", dev.speed)
                } else {
                    "Connected".into()
                }
            } else if dev.carrier.unwrap_or(false) {
                "Cable connected".into()
            } else {
                "Disconnected".into()
            };
            let info_label = gtk::Label::new(Some(&info));
            info_label.add_css_class("net-dim");
            row.append(&info_label);

            let btn = gtk::Button::new();
            btn.set_child(Some(&row));
            btn.add_css_class("flat");
            btn.add_css_class("net-device-btn");
            btn.set_sensitive(false);
            self.eth_device_box.append(&btn);
        }
    }

    fn update_vpn_list(&mut self, vpns: Vec<VpnEntry>) {
        self.vpn_section.set_visible(!vpns.is_empty());

        let visible_uuids: std::collections::HashSet<&str> =
            vpns.iter().map(|v| v.uuid.as_str()).collect();
        let to_remove: Vec<String> = self.vpn_rows.keys()
            .filter(|uuid| !visible_uuids.contains(uuid.as_str()))
            .cloned()
            .collect();
        for uuid in to_remove {
            if let Some(row) = self.vpn_rows.remove(&uuid) {
                self.vpn_box.remove(&row.button);
            }
        }

        for (i, vpn) in vpns.iter().enumerate() {
            if let Some(row) = self.vpn_rows.get(&vpn.uuid) {
                update_vpn_row(row, vpn);
                if i == 0 {
                    self.vpn_box.reorder_child_after(&row.button, Option::<&gtk::Widget>::None);
                } else if let Some(prev) = vpns.get(i - 1) {
                    if let Some(prev_row) = self.vpn_rows.get(&prev.uuid) {
                        self.vpn_box.reorder_child_after(&row.button, Some(&prev_row.button));
                    }
                }
            } else {
                let row = create_vpn_row(vpn, &self.client);
                self.vpn_box.append(&row.button);
                self.vpn_rows.insert(vpn.uuid.clone(), row);
            }
        }
    }
}

fn create_ap_row(ap: &WifiAp, client: &Arc<Client>) -> ApRow {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

    let icon = gtk::Image::from_icon_name(signal_icon_name(ap.strength));
    icon.set_pixel_size(16);
    icon.set_valign(gtk::Align::Center);
    row.append(&icon);

    let name_label = gtk::Label::new(Some(&ap.ssid));
    name_label.set_hexpand(true);
    name_label.set_halign(gtk::Align::Start);
    name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name_label.set_max_width_chars(25);
    row.append(&name_label);

    let right_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    right_box.set_valign(gtk::Align::Center);

    let lock_icon = gtk::Image::from_icon_name("channel-secure-symbolic");
    lock_icon.set_pixel_size(12);
    lock_icon.add_css_class("net-lock-icon");
    lock_icon.set_visible(!ap.security.is_empty() && ap.security != "open" && !ap.connected);
    right_box.append(&lock_icon);

    let spinner = gtk::Spinner::new();
    spinner.set_visible(false);
    spinner.set_size_request(16, 16);
    right_box.append(&spinner);

    row.append(&right_box);

    let btn = gtk::Button::new();
    btn.set_child(Some(&row));
    btn.add_css_class("flat");
    btn.add_css_class("net-ap-btn");

    let connecting = Rc::new(Cell::new(false));
    let connected = Rc::new(Cell::new(ap.connected));

    apply_ap_icon_style(&icon, ap.connected);

    // Left click
    let c = client.clone();
    let ssid = ap.ssid.clone();
    let uuid = ap.uuid.clone();
    let is_saved = ap.saved;
    let is_encrypted = !ap.security.is_empty() && ap.security != "open";
    let conn_flag = connecting.clone();
    let conn_state = connected.clone();
    let spin = spinner.clone();
    btn.connect_clicked(move |_| {
        if conn_flag.get() { return; }
        if conn_state.get() {
            if let Some(ref u) = uuid {
                spawn_connect_call(
                    &c, "network.disconnect", serde_json::json!({"uuid": u}),
                    ssid.clone(), conn_flag.clone(), spin.clone(),
                );
            }
        } else if is_saved {
            if let Some(ref u) = uuid {
                spawn_connect_call(
                    &c, "network.connect_uuid", serde_json::json!({"uuid": u}),
                    ssid.clone(), conn_flag.clone(), spin.clone(),
                );
            }
        } else if is_encrypted {
            spawn_password_dialog(&c, ssid.clone(), conn_flag.clone(), spin.clone());
        } else {
            spawn_connect_call(
                &c, "network.connect", serde_json::json!({"ssid": ssid}),
                ssid.clone(), conn_flag.clone(), spin.clone(),
            );
        }
    });

    // Right-click context menu for saved APs
    let mut popover_menu = None;
    if ap.saved {
        let menu = gtk::gio::Menu::new();
        menu.append(Some("Forget"), Some("net.forget"));

        let pm = gtk::PopoverMenu::from_model(Some(&menu));
        pm.set_parent(&btn);
        pm.set_has_arrow(false);

        let action_group = gtk::gio::SimpleActionGroup::new();
        if let Some(ref u) = ap.uuid {
            let c = client.clone();
            let uuid = u.clone();
            let action = gtk::gio::SimpleAction::new("forget", None);
            action.connect_activate(move |_, _| {
                spawn_call(&c, "network.forget", serde_json::json!({"uuid": uuid}));
            });
            action_group.add_action(&action);
        }
        btn.insert_action_group("net", Some(&action_group));

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        let pm_ref = pm.clone();
        right_click.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            pm_ref.popup();
        });
        btn.add_controller(right_click);

        popover_menu = Some(pm);
    }

    btn.set_tooltip_text(Some(&ap_tooltip(ap)));

    ApRow {
        button: btn, icon, name_label, lock_icon, spinner,
        popover_menu, connecting, connected,
    }
}

fn update_ap_row(row: &ApRow, ap: &WifiAp) {
    if row.connecting.get() {
        // Connection/disconnection in progress — check if it completed
        let was_connected = row.connected.get();
        if ap.connected != was_connected {
            // State changed — operation completed
            row.connecting.set(false);
            row.spinner.stop();
            row.spinner.set_visible(false);
        } else {
            return; // Still in progress
        }
    }
    row.connected.set(ap.connected);
    row.icon.set_icon_name(Some(signal_icon_name(ap.strength)));
    apply_ap_icon_style(&row.icon, ap.connected);
    row.name_label.set_label(&ap.ssid);
    row.lock_icon.set_visible(!ap.security.is_empty() && ap.security != "open" && !ap.connected);

    row.button.set_tooltip_text(Some(&ap_tooltip(ap)));
}

fn apply_ap_icon_style(icon: &gtk::Image, connected: bool) {
    if connected {
        icon.remove_css_class("net-ap-icon");
        icon.add_css_class("net-ap-icon-active");
    } else {
        icon.remove_css_class("net-ap-icon-active");
        icon.add_css_class("net-ap-icon");
    }
}

fn create_vpn_row(vpn: &VpnEntry, client: &Arc<Client>) -> VpnRow {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

    let icon = gtk::Image::from_icon_name("network-vpn-symbolic");
    icon.set_pixel_size(16);
    icon.set_valign(gtk::Align::Center);
    icon.add_css_class("net-ap-icon");
    row.append(&icon);

    let name_label = gtk::Label::new(Some(&vpn.id));
    name_label.set_hexpand(true);
    name_label.set_halign(gtk::Align::Start);
    name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name_label.set_max_width_chars(25);
    row.append(&name_label);

    let spinner = gtk::Spinner::new();
    spinner.set_visible(false);
    spinner.set_size_request(16, 16);
    row.append(&spinner);

    let btn = gtk::Button::new();
    btn.set_child(Some(&row));
    btn.add_css_class("flat");
    btn.add_css_class("net-vpn-btn");

    let connecting = Rc::new(Cell::new(false));
    let active = Rc::new(Cell::new(vpn.active));

    let c = client.clone();
    let uuid = vpn.uuid.clone();
    let name = vpn.id.clone();
    let conn_flag = connecting.clone();
    let active_flag = active.clone();
    let spin = spinner.clone();
    btn.connect_clicked(move |_| {
        if conn_flag.get() { return; }
        if active_flag.get() {
            spawn_connect_call(
                &c, "network.disconnect", serde_json::json!({"uuid": uuid}),
                name.clone(), conn_flag.clone(), spin.clone(),
            );
        } else {
            spawn_connect_call(
                &c, "network.connect_uuid", serde_json::json!({"uuid": uuid}),
                name.clone(), conn_flag.clone(), spin.clone(),
            );
        }
    });

    let tooltip = if vpn.active { "Disconnect VPN" } else { "Connect VPN" };
    btn.set_tooltip_text(Some(tooltip));

    VpnRow { button: btn, spinner, connecting, active }
}

fn update_vpn_row(row: &VpnRow, vpn: &VpnEntry) {
    if row.connecting.get() {
        let was_active = row.active.get();
        if vpn.active != was_active {
            row.connecting.set(false);
            row.spinner.stop();
            row.spinner.set_visible(false);
        } else {
            return;
        }
    }
    row.active.set(vpn.active);
    let tooltip = if vpn.active { "Disconnect VPN" } else { "Connect VPN" };
    row.button.set_tooltip_text(Some(tooltip));
}
