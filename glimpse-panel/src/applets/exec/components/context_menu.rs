use relm4::{ComponentSender, gtk::{self, prelude::*}};

use crate::applets::exec::applet::{Exec, ExecMsg};

pub fn build_context_menu(root: &gtk::Box, sender: &ComponentSender<Exec>) -> gtk::PopoverMenu {
    let action_group = gtk::gio::SimpleActionGroup::new();
    let restart_action = gtk::gio::SimpleAction::new("restart_command", None);
    restart_action.connect_activate({
        let sender = sender.input_sender().clone();
        move |_, _| sender.emit(ExecMsg::RestartCommand)
    });
    action_group.add_action(&restart_action);
    root.insert_action_group("exec", Some(&action_group));

    let menu = gtk::gio::Menu::new();
    menu.append(Some("Restart command"), Some("exec.restart_command"));
    let context_menu = gtk::PopoverMenu::from_model(Some(&menu));
    context_menu.set_parent(root);
    context_menu.set_has_arrow(false);
    {
        let context_menu = context_menu.clone();
        root.connect_destroy(move |_| context_menu.unparent());
    }

    let right_click = gtk::GestureClick::new();
    right_click.set_button(3);
    {
        let context_menu = context_menu.clone();
        right_click.connect_pressed(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            context_menu.popup();
        });
    }
    root.add_controller(right_click);

    context_menu
}
