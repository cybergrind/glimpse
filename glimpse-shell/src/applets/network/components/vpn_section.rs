#![allow(unused_assignments)]

use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::{
    components::action_menu::{
        ActionMenu, ActionMenuItem, Init as ActionMenuInit, Input as ActionMenuInput,
    },
    services::network::{Command, SavedVpn},
};

pub struct VpnSection {
    menu: Controller<ActionMenu<Command>>,
    items: Vec<ActionMenuItem<Command>>,
}

#[derive(Debug)]
pub enum VpnSectionInput {
    Update(Vec<SavedVpn>),
    MenuCommand(Command),
}

#[relm4::component(pub)]
impl SimpleComponent for VpnSection {
    type Init = ();
    type Input = VpnSectionInput;
    type Output = Command;

    view! {
        root = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,

            #[local_ref]
            menu_widget -> gtk::Box {},
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let menu = ActionMenu::builder()
            .launch(ActionMenuInit {
                header: Some("VPN".into()),
                items: Vec::new(),
            })
            .forward(sender.input_sender(), VpnSectionInput::MenuCommand);
        let menu_widget = menu.widget().clone();
        menu.widget().set_visible(false);

        let model = VpnSection {
            menu,
            items: Vec::new(),
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            VpnSectionInput::Update(vpns) => {
                let items = build_vpn_items(&vpns);
                if self.items != items {
                    self.menu.widget().set_visible(!items.is_empty());
                    self.menu.emit(ActionMenuInput::Update(items.clone()));
                    self.items = items;
                }
            }
            VpnSectionInput::MenuCommand(command) => {
                let _ = sender.output(command);
            }
        }
    }
}

fn build_vpn_items(vpns: &[SavedVpn]) -> Vec<ActionMenuItem<Command>> {
    vpns.iter()
        .filter(|vpn| !vpn.id.is_empty() && !vpn.uuid.is_empty())
        .map(|vpn| ActionMenuItem {
            label: vpn.id.clone(),
            icon: Some("network-vpn-symbolic".into()),
            visible: true,
            checked: Some(vpn.active),
            selectable: Some(true),
            command: primary_vpn_command(vpn),
        })
        .collect()
}

fn primary_vpn_command(vpn: &SavedVpn) -> Command {
    if vpn.active {
        Command::Disconnect {
            uuid: vpn.uuid.clone(),
        }
    } else {
        Command::ConnectSaved {
            uuid: vpn.uuid.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vpn_items_toggle_active_connections() {
        let items = build_vpn_items(&[SavedVpn {
            id: "Work".into(),
            uuid: "vpn-1".into(),
            active: true,
            ..SavedVpn::default()
        }]);

        assert_eq!(items[0].label, "Work");
        assert_eq!(items[0].checked, Some(true));
        assert_eq!(
            items[0].command,
            Command::Disconnect {
                uuid: "vpn-1".into()
            }
        );
    }
}
