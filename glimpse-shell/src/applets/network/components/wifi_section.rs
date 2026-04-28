#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    applets::network::format,
    components::device_list::{DeviceList, DeviceListInit, DeviceListInput, DeviceListItem},
    services::network::{Command, NetworkActiveAction, WifiAccessPoint},
};

pub struct WifiSection {
    empty_visible: bool,
    list: Controller<DeviceList<Command>>,
    items: Vec<DeviceListItem<Command>>,
}

#[derive(Debug)]
pub enum WifiSectionInput {
    Update {
        access_points: Vec<WifiAccessPoint>,
        wifi_enabled: bool,
        active_action: Option<NetworkActiveAction>,
    },
    DeviceCommand(Command),
}

#[relm4::component(pub)]
impl SimpleComponent for WifiSection {
    type Init = ();
    type Input = WifiSectionInput;
    type Output = Command;

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            #[name = "empty_label"]
            gtk::Label {
                set_label: "No access points",
                set_halign: gtk::Align::Start,
                add_css_class: "net-empty",
                add_css_class: "empty-state__subtitle",
                #[watch]
                set_visible: model.empty_visible,
            },

            #[local_ref]
            list_widget -> gtk::Box {},
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let list = DeviceList::builder()
            .launch(DeviceListInit {
                header: None,
                items: Vec::new(),
            })
            .forward(sender.input_sender(), WifiSectionInput::DeviceCommand);
        let list_widget = list.widget().clone();
        list.widget().set_visible(false);

        let model = WifiSection {
            empty_visible: false,
            list,
            items: Vec::new(),
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            WifiSectionInput::Update {
                access_points,
                wifi_enabled,
                active_action,
            } => {
                let items = if wifi_enabled {
                    build_wifi_items(&access_points, active_action.as_ref())
                } else {
                    Vec::new()
                };
                if self.items != items {
                    self.list.widget().set_visible(!items.is_empty());
                    self.list.emit(DeviceListInput::Update(items.clone()));
                    self.items = items;
                }
                self.empty_visible = wifi_enabled && self.items.is_empty();
            }
            WifiSectionInput::DeviceCommand(command) => {
                if let Some(id) = optimistic_busy_id(&command).map(str::to_owned) {
                    if mark_wifi_busy(&mut self.items, &id) {
                        self.list.emit(DeviceListInput::Update(self.items.clone()));
                    }
                }
                let _ = sender.output(command);
            }
        }
    }
}

fn build_wifi_items(
    access_points: &[WifiAccessPoint],
    active_action: Option<&NetworkActiveAction>,
) -> Vec<DeviceListItem<Command>> {
    access_points
        .iter()
        .filter(|access_point| is_visible_access_point(access_point))
        .map(|access_point| {
            let id = wifi_item_id(access_point);
            DeviceListItem {
                id: id.clone(),
                label: access_point.ssid.clone(),
                icon: format::wifi_icon(access_point.strength).into(),
                status: wifi_status(access_point),
                busy: is_wifi_busy(active_action, access_point),
                tooltip: Some(access_point_tooltip(access_point)),
                active: access_point.connected,
                visible: true,
                command: Some(primary_wifi_command(access_point)),
            }
        })
        .collect()
}

fn primary_wifi_command(access_point: &WifiAccessPoint) -> Command {
    if access_point.connected {
        if let Some(uuid) = &access_point.uuid {
            return Command::Disconnect { uuid: uuid.clone() };
        }
    }

    if access_point.saved {
        if let Some(uuid) = &access_point.uuid {
            return Command::ConnectSaved { uuid: uuid.clone() };
        }
    }

    Command::ConnectWifi {
        ssid: access_point.ssid.clone(),
        path: access_point.path.clone(),
    }
}

fn wifi_status(access_point: &WifiAccessPoint) -> String {
    if access_point.connected {
        format::wifi_status(access_point)
    } else {
        String::new()
    }
}

fn is_wifi_busy(
    active_action: Option<&NetworkActiveAction>,
    access_point: &WifiAccessPoint,
) -> bool {
    match active_action {
        Some(NetworkActiveAction::ConnectWifi { path, .. }) => path == &access_point.path,
        Some(NetworkActiveAction::ConnectSaved { uuid })
        | Some(NetworkActiveAction::Disconnect { uuid }) => {
            access_point.uuid.as_deref() == Some(uuid)
        }
        Some(NetworkActiveAction::SetWifiEnabled(_))
        | Some(NetworkActiveAction::Forget { .. })
        | None => false,
    }
}

fn optimistic_busy_id(command: &Command) -> Option<&str> {
    match command {
        Command::ConnectWifi { path, .. } => Some(path.as_str()),
        Command::ConnectSaved { uuid } | Command::Disconnect { uuid } => Some(uuid.as_str()),
        Command::SetWifiEnabled(_)
        | Command::StartScanning { .. }
        | Command::StopScanning
        | Command::RequestScan
        | Command::Forget { .. }
        | Command::PromptReply { .. } => None,
    }
}

fn mark_wifi_busy(items: &mut [DeviceListItem<Command>], id: &str) -> bool {
    let mut changed = false;
    for item in items {
        let busy = item.id == id || matches_wifi_command_uuid(&item.command, id);
        if item.busy != busy {
            item.busy = busy;
            changed = true;
        }
    }
    changed
}

fn matches_wifi_uuid(command: &Command, uuid: &str) -> bool {
    match command {
        Command::ConnectSaved { uuid: command_uuid }
        | Command::Disconnect { uuid: command_uuid } => command_uuid == uuid,
        _ => false,
    }
}

fn matches_wifi_command_uuid(command: &Option<Command>, uuid: &str) -> bool {
    command
        .as_ref()
        .is_some_and(|command| matches_wifi_uuid(command, uuid))
}

fn wifi_item_id(access_point: &WifiAccessPoint) -> String {
    access_point.path.clone()
}

fn is_visible_access_point(access_point: &WifiAccessPoint) -> bool {
    !access_point.ssid.is_empty()
}

fn access_point_tooltip(access_point: &WifiAccessPoint) -> String {
    let mut parts = Vec::new();
    if access_point.connected {
        parts.push("Connected".into());
    } else if access_point.saved {
        parts.push("Saved".into());
    }

    if !access_point.security.is_empty() && access_point.security != "open" {
        parts.push(access_point.security.to_uppercase());
    }

    if access_point.frequency > 0 {
        parts.push(frequency_text(access_point.frequency));
    }

    parts.push(format!("{}%", access_point.strength));
    parts.join(" - ")
}

fn frequency_text(frequency_mhz: u32) -> String {
    if frequency_mhz >= 1000 {
        format!("{:.1} GHz", frequency_mhz as f32 / 1000.0)
    } else {
        format!("{frequency_mhz} MHz")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access_point(ssid: &str, strength: u8, connected: bool, saved: bool) -> WifiAccessPoint {
        WifiAccessPoint {
            path: format!("/ap/{ssid}"),
            ssid: ssid.into(),
            strength,
            security: "wpa2".into(),
            connected,
            saved,
            uuid: saved.then(|| format!("uuid-{ssid}")),
            ..WifiAccessPoint::default()
        }
    }

    #[test]
    fn primary_wifi_command_matches_access_point_state() {
        assert_eq!(
            primary_wifi_command(&access_point("Home", 80, true, true)),
            Command::Disconnect {
                uuid: "uuid-Home".into()
            }
        );
        assert_eq!(
            primary_wifi_command(&access_point("Home", 80, false, true)),
            Command::ConnectSaved {
                uuid: "uuid-Home".into()
            }
        );
        assert_eq!(
            primary_wifi_command(&access_point("Cafe", 80, false, false)),
            Command::ConnectWifi {
                ssid: "Cafe".into(),
                path: "/ap/Cafe".into()
            }
        );
    }

    #[test]
    fn wifi_items_use_signal_strength_as_status() {
        let items = build_wifi_items(&[access_point("Home", 72, true, true)], None);

        assert_eq!(items[0].status, "72%");
        assert!(items[0].active);
    }

    #[test]
    fn wifi_items_hide_signal_strength_for_inactive_rows() {
        let items = build_wifi_items(&[access_point("Cafe", 72, false, false)], None);

        assert_eq!(items[0].status, "");
        assert!(!items[0].active);
    }

    #[test]
    fn wifi_items_keep_access_point_paths_as_row_ids() {
        let mut first = access_point("Home", 72, false, true);
        first.path = "/ap/1".into();
        first.uuid = Some("home-profile".into());
        let mut second = access_point("Home", 68, false, true);
        second.path = "/ap/2".into();
        second.uuid = Some("home-profile".into());

        let items = build_wifi_items(&[first, second], None);

        assert_eq!(items[0].id, "/ap/1");
        assert_eq!(items[1].id, "/ap/2");
    }

    #[test]
    fn connecting_wifi_item_sets_busy_status_slot() {
        let items = build_wifi_items(
            &[access_point("Cafe", 40, false, false)],
            Some(&NetworkActiveAction::ConnectWifi {
                ssid: "Cafe".into(),
                path: "/ap/Cafe".into(),
            }),
        );

        assert!(items[0].busy);
    }
}
