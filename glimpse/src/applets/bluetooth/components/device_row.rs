use std::time::Duration;

use relm4::{
    ComponentParts, ComponentSender, SimpleComponent,
    gtk::{self, glib, prelude::*},
};

use super::{BluetoothDeviceAction, BtDevice};

pub struct BluetoothDeviceRow {
    device: BtDevice,
    tooltip: String,
    battery_text: String,
    battery_visible: bool,
    icon: gtk::Image,
    spinner: gtk::Spinner,
    popover_menu: gtk::PopoverMenu,
    connecting: bool,
    pending_action: Option<BluetoothDeviceAction>,
    action_timeout: Option<glib::SourceId>,
}

#[derive(Debug)]
pub enum BluetoothDeviceRowInput {
    Update(BtDevice),
    Activate,
    StartAction(BluetoothDeviceAction),
    FinishAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothDeviceRowOutput {
    Action {
        address: String,
        name: String,
        action: BluetoothDeviceAction,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for BluetoothDeviceRow {
    type Init = BtDevice;
    type Input = BluetoothDeviceRowInput;
    type Output = BluetoothDeviceRowOutput;

    view! {
        root = gtk::Button {
            add_css_class: "flat",
            add_css_class: "bt-device-btn",
            #[watch]
            set_tooltip_text: Some(&model.tooltip),
            connect_clicked => BluetoothDeviceRowInput::Activate,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "bt-device-row",

                #[name(icon)]
                gtk::Image {
                    #[watch]
                    set_icon_name: Some(&model.device.icon),
                    set_pixel_size: 16,
                    set_valign: gtk::Align::Center,
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.device.name,
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_max_width_chars: 25,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                    set_valign: gtk::Align::Center,

                    gtk::Label {
                        #[watch]
                        set_visible: model.battery_visible,
                        #[watch]
                        set_label: &model.battery_text,
                        add_css_class: "bt-battery",
                    },

                    #[name(spinner)]
                    gtk::Spinner {
                        #[watch]
                        set_visible: model.connecting,
                        set_valign: gtk::Align::Center,
                    },
                },
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.add_css_class("bt-device-btn");

        let model = BluetoothDeviceRow {
            tooltip: device_tooltip(&init),
            battery_text: battery_text(init.battery),
            battery_visible: init.battery.is_some(),
            device: init.clone(),
            icon: gtk::Image::new(),
            spinner: gtk::Spinner::new(),
            popover_menu: gtk::PopoverMenu::from_model(None::<&gtk::gio::MenuModel>),
            connecting: false,
            pending_action: None,
            action_timeout: None,
        };

        let widgets = view_output!();
        widgets.icon.set_pixel_size(16);
        widgets.icon.set_valign(gtk::Align::Center);
        apply_icon_style(&widgets.icon, model.device.connected);
        widgets.spinner.set_visible(false);
        widgets.spinner.set_valign(gtk::Align::Center);

        let button = widgets.root.clone();
        let menu_model = build_menu(model.device.connected, model.device.paired, model.device.trusted);
        let popover_menu = gtk::PopoverMenu::from_model(Some(&menu_model));
        popover_menu.set_parent(&button);
        popover_menu.set_has_arrow(false);
        {
            let popover_menu = popover_menu.clone();
            button.connect_destroy(move |_| {
                popover_menu.popdown();
                popover_menu.unparent();
            });
        }

        let action_group = gtk::gio::SimpleActionGroup::new();
        setup_actions(
            &action_group,
            sender.clone(),
            &model.device.address,
            &model.device.name,
        );
        button.insert_action_group("bt", Some(&action_group));

        let right_click = gtk::GestureClick::new();
        right_click.set_button(3);
        let popover = popover_menu.clone();
        right_click.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            popover.popup();
        });
        button.add_controller(right_click);

        let mut model = model;
        model.icon = widgets.icon.clone();
        model.spinner = widgets.spinner.clone();
        model.popover_menu = popover_menu;

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BluetoothDeviceRowInput::Update(device) => {
                self.device = device;
                self.tooltip = device_tooltip(&self.device);
                self.battery_text = battery_text(self.device.battery);
                self.battery_visible = self.device.battery.is_some();
                self.popover_menu
                    .set_menu_model(Some(&build_menu(
                        self.device.connected,
                        self.device.paired,
                        self.device.trusted,
                    )));
                apply_icon_style(&self.icon, self.device.connected);
                if self.connecting
                    && self
                        .pending_action
                        .is_some_and(|action| action_observed_complete(action, &self.device))
                {
                    self.finish_action();
                }
            }
            BluetoothDeviceRowInput::Activate => {
                if self.connecting {
                    tracing::debug!(
                        address = %self.device.address,
                        "bluetooth ui: ignoring click while action pending"
                    );
                    return;
                }
                let action = primary_action(&self.device);
                self.start_action(action, sender);
            }
            BluetoothDeviceRowInput::StartAction(action) => {
                if self.connecting {
                    tracing::debug!(
                        address = %self.device.address,
                        ?action,
                        "bluetooth ui: ignoring action while pending"
                    );
                    return;
                }
                self.start_action(action, sender);
            }
            BluetoothDeviceRowInput::FinishAction => {
                self.finish_action();
            }
        }
    }
}

impl BluetoothDeviceRow {
    fn start_action(
        &mut self,
        action: BluetoothDeviceAction,
        sender: ComponentSender<Self>,
    ) {
        tracing::info!(
            ?action,
            address = %self.device.address,
            name = %self.device.name,
            "bluetooth ui: device action clicked"
        );
        self.connecting = true;
        self.pending_action = Some(action);
        self.spinner.set_visible(true);
        self.spinner.start();
        let timeout_sender = sender.clone();
        self.action_timeout = Some(glib::timeout_add_local_once(
            Duration::from_secs(15),
            move || {
                let _ = timeout_sender.input(BluetoothDeviceRowInput::FinishAction);
            },
        ));
        let _ = sender.output(BluetoothDeviceRowOutput::Action {
            address: self.device.address.clone(),
            name: self.device.name.clone(),
            action,
        });
    }

    fn finish_action(&mut self) {
        if let Some(source) = self.action_timeout.take() {
            source.remove();
        }
        self.connecting = false;
        self.pending_action = None;
        self.spinner.stop();
        self.spinner.set_visible(false);
    }
}

fn build_menu(connected: bool, paired: bool, trusted: bool) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    if connected {
        menu.append(Some("Disconnect"), Some("bt.disconnect"));
    } else {
        menu.append(Some("Connect"), Some("bt.connect"));
    }
    if !paired {
        menu.append(Some("Pair"), Some("bt.pair"));
    } else if trusted {
        menu.append(Some("Untrust"), Some("bt.untrust"));
    } else {
        menu.append(Some("Trust"), Some("bt.trust"));
    }
    menu.append(Some("Forget"), Some("bt.forget"));
    menu
}

fn setup_actions(
    group: &gtk::gio::SimpleActionGroup,
    sender: ComponentSender<BluetoothDeviceRow>,
    address: &str,
    name: &str,
) {
    for (action_name, action_kind) in [
        ("disconnect", BluetoothDeviceAction::Disconnect),
        ("connect", BluetoothDeviceAction::Connect),
        ("pair", BluetoothDeviceAction::Pair),
        ("trust", BluetoothDeviceAction::Trust(true)),
        ("untrust", BluetoothDeviceAction::Trust(false)),
    ] {
        let addr = address.to_owned();
        let dev_name = name.to_owned();
        let sender = sender.clone();
        let action = gtk::gio::SimpleAction::new(action_name, None);
        action.connect_activate(move |_, _| {
            let _ = sender.input(BluetoothDeviceRowInput::StartAction(action_kind));
            tracing::debug!(address = %addr, name = %dev_name, ?action_kind, "bluetooth ui: menu action activated");
        });
        group.add_action(&action);
    }

    let addr = address.to_owned();
    let dev_name = name.to_owned();
    let sender = sender.clone();
    let action = gtk::gio::SimpleAction::new("forget", None);
    action.connect_activate(move |_, _| {
        tracing::debug!(address = %addr, name = %dev_name, "bluetooth ui: forget action activated");
        let _ = sender.input(BluetoothDeviceRowInput::StartAction(BluetoothDeviceAction::Forget));
    });
    group.add_action(&action);
}

fn primary_action(device: &BtDevice) -> BluetoothDeviceAction {
    if device.connected {
        BluetoothDeviceAction::Disconnect
    } else if device.paired {
        BluetoothDeviceAction::Connect
    } else {
        BluetoothDeviceAction::Pair
    }
}

fn action_observed_complete(action: BluetoothDeviceAction, dev: &BtDevice) -> bool {
    match action {
        BluetoothDeviceAction::Connect => dev.connected,
        BluetoothDeviceAction::Disconnect => !dev.connected,
        BluetoothDeviceAction::Pair => dev.paired,
        BluetoothDeviceAction::Trust(trusted) => dev.trusted == trusted,
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

fn device_tooltip(dev: &BtDevice) -> String {
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
    if parts.is_empty() {
        dev.name.clone()
    } else {
        parts.join(" \u{b7} ")
    }
}

fn battery_text(battery: Option<u8>) -> String {
    battery.map(|pct| format!("{pct}%")).unwrap_or_default()
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
        assert!(action_observed_complete(
            BluetoothDeviceAction::Trust(true),
            &device(false, true, true)
        ));
        assert!(action_observed_complete(
            BluetoothDeviceAction::Trust(false),
            &device(false, true, false)
        ));
        assert!(!action_observed_complete(
            BluetoothDeviceAction::Forget,
            &device(false, false, true)
        ));
    }

    #[test]
    fn tooltip_includes_device_metadata() {
        let mut device = device(false, false, false);
        device.device_type = "Headphones".into();
        device.battery = Some(75);
        assert_eq!(device_tooltip(&device), "Headphones · 75%");
    }
}
