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
pub struct NotifData {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub desktop_entry: Option<String>,
    pub summary: String,
    pub body: String,
    pub urgency: u8,
    pub actions: Vec<(String, String)>,
    pub image: Option<String>,
    pub timestamp: u64,
}

pub struct NotificationsPopover {
    popover: gtk::Popover,
    client: Arc<Client>,
    hero_icon: gtk::Image,
    subtitle: gtk::Label,
    dnd_switch: gtk::Switch,
    notif_box: gtk::Box,
    empty_label: gtk::Label,
    updating_dnd: Rc<Cell<bool>>,
    dnd: bool,
    count: u32,
    /// Stacking state: true = stacked (collapsed), false = expanded
    stack_state: HashMap<String, bool>,
    last_notifications: Vec<NotifData>,
}

pub struct NotificationsPopoverInit {
    pub parent: gtk::Box,
    pub client: Arc<Client>,
}

#[derive(Debug)]
pub enum NotificationsPopoverInput {
    Toggle,
    UpdateStatus {
        dnd: bool,
        count: u32,
        badge_count: u32,
    },
    UpdateList(serde_json::Value),
    ToggleStack(String),
}

/// Resolve notification icon: try app_icon, then desktop_entry, then app_name lowercased
fn resolve_notif_icon(notif: &NotifData) -> &str {
    if !notif.app_icon.is_empty() {
        return &notif.app_icon;
    }
    if let Some(ref de) = notif.desktop_entry {
        if !de.is_empty() {
            return de;
        }
    }
    "dialog-information-symbolic"
}

fn spawn_call(client: &Arc<Client>, method: &'static str, params: serde_json::Value) {
    let c = client.clone();
    glib::spawn_future_local(async move {
        let _ = c.call(method, params).await;
    });
}

fn format_time_diff(diff_secs: u64) -> String {
    match diff_secs {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", diff_secs / 60),
        3600..=86399 => format!("{}h", diff_secs / 3600),
        86400..=172799 => "yesterday".into(),
        _ => format!("{}d", diff_secs / 86400),
    }
}

fn time_ago(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    format_time_diff(now.saturating_sub(timestamp) / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_diff_now() {
        assert_eq!(format_time_diff(0), "now");
        assert_eq!(format_time_diff(30), "now");
        assert_eq!(format_time_diff(59), "now");
    }

    #[test]
    fn time_diff_minutes() {
        assert_eq!(format_time_diff(60), "1m");
        assert_eq!(format_time_diff(120), "2m");
        assert_eq!(format_time_diff(3599), "59m");
    }

    #[test]
    fn time_diff_hours() {
        assert_eq!(format_time_diff(3600), "1h");
        assert_eq!(format_time_diff(7200), "2h");
        assert_eq!(format_time_diff(86399), "23h");
    }

    #[test]
    fn time_diff_yesterday() {
        assert_eq!(format_time_diff(86400), "yesterday");
        assert_eq!(format_time_diff(172799), "yesterday");
    }

    #[test]
    fn time_diff_days() {
        assert_eq!(format_time_diff(172800), "2d");
        assert_eq!(format_time_diff(604800), "7d");
    }
}

impl SimpleComponent for NotificationsPopover {
    type Init = NotificationsPopoverInit;
    type Input = NotificationsPopoverInput;
    type Output = ();
    type Root = gtk::Popover;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Popover::new()
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_parent(&init.parent);
        root.set_autohide(true);
        root.add_css_class("notifications-popover");

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.set_hexpand(false);
        vbox.set_overflow(gtk::Overflow::Hidden);

        // === Hero ===
        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero.add_css_class("notif-hero");

        let hero_icon = gtk::Image::from_icon_name("preferences-system-notifications-symbolic");
        hero_icon.set_pixel_size(32);
        hero.append(&hero_icon);

        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        title_box.set_hexpand(true);
        title_box.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some("Notifications"));
        title.set_halign(gtk::Align::Start);
        title.add_css_class("notif-title");
        title_box.append(&title);
        let subtitle = gtk::Label::new(Some("No notifications"));
        subtitle.set_halign(gtk::Align::Start);
        subtitle.add_css_class("notif-subtitle");
        title_box.append(&subtitle);
        hero.append(&title_box);

        let dnd_switch = gtk::Switch::new();
        dnd_switch.set_active(true); // notifications ON by default
        dnd_switch.set_valign(gtk::Align::Center);
        dnd_switch.set_tooltip_text(Some("Notifications"));
        let updating_dnd = Rc::new(Cell::new(false));
        let guard = updating_dnd.clone();
        let c = init.client.clone();
        dnd_switch.connect_state_set(move |_, active| {
            if guard.get() {
                return glib::Propagation::Stop;
            }
            // active=true → notifications ON (dnd=false), active=false → DnD (dnd=true)
            spawn_call(
                &c,
                "notifications.set_dnd",
                serde_json::json!({"enabled": !active}),
            );
            glib::Propagation::Stop
        });
        hero.append(&dnd_switch);

        vbox.append(&hero);
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        // === Notification list ===
        let empty_label = gtk::Label::new(Some("No notifications"));
        empty_label.set_halign(gtk::Align::Center);
        empty_label.set_valign(gtk::Align::Center);
        empty_label.add_css_class("notif-empty");
        vbox.append(&empty_label);

        let notif_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        notif_box.add_css_class("notif-list");

        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_max_content_height(600);
        scroll.set_propagate_natural_height(true);
        scroll.set_vexpand(true);
        scroll.set_child(Some(&notif_box));
        vbox.append(&scroll);

        // === Clear All ===
        vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let clear_lbl = gtk::Label::new(Some("Clear All"));
        clear_lbl.set_halign(gtk::Align::Start);
        let clear_btn = gtk::Button::new();
        clear_btn.set_child(Some(&clear_lbl));
        clear_btn.add_css_class("flat");
        clear_btn.add_css_class("settings-btn");
        let c = init.client.clone();
        clear_btn.connect_clicked(move |_| {
            spawn_call(&c, "notifications.dismiss_all", serde_json::json!({}));
        });
        vbox.append(&clear_btn);

        root.set_child(Some(&vbox));

        let model = NotificationsPopover {
            popover: root.clone(),
            client: init.client,
            hero_icon,
            subtitle,
            dnd_switch,
            notif_box,
            empty_label,
            updating_dnd,
            dnd: false,
            count: 0,
            stack_state: HashMap::new(),
            last_notifications: Vec::new(),
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NotificationsPopoverInput::Toggle => {
                if self.popover.is_visible() {
                    self.popover.popdown();
                } else {
                    self.popover.popup();
                }
            }
            NotificationsPopoverInput::UpdateStatus {
                dnd,
                count,
                badge_count: _,
            } => {
                self.dnd = dnd;
                self.count = count;

                // Switch: active=true means notifications ON (dnd=false)
                let switch_active = !dnd;
                if self.dnd_switch.is_active() != switch_active {
                    self.updating_dnd.set(true);
                    self.dnd_switch.set_active(switch_active);
                    self.dnd_switch.set_state(switch_active);
                    self.updating_dnd.set(false);
                }

                self.hero_icon.set_icon_name(Some(if dnd {
                    "notifications-disabled-symbolic"
                } else {
                    "preferences-system-notifications-symbolic"
                }));

                self.subtitle.set_label(&if count == 0 {
                    "No notifications".into()
                } else {
                    format!("{count} notification{}", if count > 1 { "s" } else { "" })
                });
            }
            NotificationsPopoverInput::UpdateList(data) => {
                let notifications: Vec<NotifData> =
                    serde_json::from_value(data).unwrap_or_default();
                self.last_notifications = notifications.clone();
                self.rebuild_list(notifications, &sender);
            }
            NotificationsPopoverInput::ToggleStack(app_name) => {
                let current = *self.stack_state.get(&app_name).unwrap_or(&true);
                self.stack_state.insert(app_name, !current);
                let notifs = self.last_notifications.clone();
                self.rebuild_list(notifs, &sender);
            }
        }
    }
}

impl NotificationsPopover {
    fn rebuild_list(&mut self, notifications: Vec<NotifData>, sender: &ComponentSender<Self>) {
        // Clear existing
        let mut child = self.notif_box.first_child();
        while let Some(w) = child {
            child = w.next_sibling();
            self.notif_box.remove(&w);
        }

        self.empty_label.set_visible(notifications.is_empty());

        if notifications.is_empty() {
            return;
        }

        // Group by app_name
        let mut groups: Vec<(String, Vec<&NotifData>)> = Vec::new();
        let mut group_map: HashMap<String, usize> = HashMap::new();
        for notif in &notifications {
            let key = if notif.app_name.is_empty() {
                "Unknown".to_string()
            } else {
                notif.app_name.clone()
            };
            if let Some(&idx) = group_map.get(&key) {
                groups[idx].1.push(notif);
            } else {
                let idx = groups.len();
                group_map.insert(key.clone(), idx);
                groups.push((key, vec![notif]));
            }
        }

        // Sort groups: most recent first
        groups.sort_by(|a, b| {
            let a_ts = a.1.iter().map(|n| n.timestamp).max().unwrap_or(0);
            let b_ts = b.1.iter().map(|n| n.timestamp).max().unwrap_or(0);
            b_ts.cmp(&a_ts)
        });

        for (app_name, notifs) in &groups {
            let is_stack = notifs.len() > 1;
            let stacked = is_stack && *self.stack_state.get(app_name).unwrap_or(&true);

            if is_stack {
                // Group container (GNOME-style)
                let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
                group.add_css_class("notif-group");

                // Group header: icon + app name (count) + expand/collapse chevron
                let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                header.add_css_class("notif-group-header");

                let icon = gtk::Image::from_icon_name(resolve_notif_icon(notifs[0]));
                icon.set_pixel_size(16);
                icon.add_css_class("notif-icon");
                header.append(&icon);

                let app_label =
                    gtk::Label::new(Some(&format!("{app_name} ({count})", count = notifs.len())));
                app_label.set_halign(gtk::Align::Start);
                app_label.set_hexpand(true);
                app_label.add_css_class("notif-app-name");
                header.append(&app_label);

                let chevron_icon = if stacked {
                    "go-down-symbolic"
                } else {
                    "go-up-symbolic"
                };
                let chevron = gtk::Button::from_icon_name(chevron_icon);
                chevron.add_css_class("flat");
                chevron.add_css_class("notif-expand-btn");
                let s = sender.clone();
                let app = app_name.clone();
                chevron.connect_clicked(move |_| {
                    s.input(NotificationsPopoverInput::ToggleStack(app.clone()));
                });
                header.append(&chevron);

                let header_btn = gtk::Button::new();
                header_btn.set_child(Some(&header));
                header_btn.add_css_class("flat");
                header_btn.add_css_class("notif-group-header-btn");
                let s = sender.clone();
                let app = app_name.clone();
                header_btn.connect_clicked(move |_| {
                    s.input(NotificationsPopoverInput::ToggleStack(app.clone()));
                });
                group.append(&header_btn);

                let mut sorted: Vec<&&NotifData> = notifs.iter().collect();
                sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

                if stacked {
                    // Collapsed: show newest card + peek depth indicators
                    let row = self.build_notification_row(sorted[0]);
                    group.append(&row);

                    // GNOME-style peek: offset cards behind
                    if notifs.len() > 1 {
                        let peek1 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        peek1.add_css_class("notif-stack-depth");
                        group.append(&peek1);
                    }
                    if notifs.len() > 2 {
                        let peek2 = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        peek2.add_css_class("notif-stack-depth-2");
                        group.append(&peek2);
                    }
                } else {
                    // Expanded: show all cards
                    let cards_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
                    cards_box.add_css_class("notif-group-cards");
                    for notif in sorted {
                        let row = self.build_notification_row(notif);
                        cards_box.append(&row);
                    }
                    group.append(&cards_box);
                }

                self.notif_box.append(&group);
            } else {
                // Single notification — no group container
                let row = self.build_notification_row(notifs[0]);
                self.notif_box.append(&row);
            }
        }
    }

    /// GNOME-style notification card layout:
    /// ┌────────────────────────────────────────┐
    /// │ (icon) App Name         2m ago    [X]  │
    /// │ Summary Title                          │
    /// │ Body text here...                      │
    /// │ [Action 1]  [Action 2]                 │
    /// └────────────────────────────────────────┘
    fn build_notification_row(&self, notif: &NotifData) -> gtk::Widget {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
        card.add_css_class("notif-card");

        // Header: icon + app name + time + dismiss
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let icon = gtk::Image::from_icon_name(resolve_notif_icon(notif));
        icon.set_pixel_size(20);
        icon.set_valign(gtk::Align::Center);
        icon.add_css_class("notif-icon");
        header.append(&icon);

        let app_name = if notif.app_name.is_empty() {
            "Notification"
        } else {
            &notif.app_name
        };
        let app_label = gtk::Label::new(Some(app_name));
        app_label.set_halign(gtk::Align::Start);
        app_label.set_hexpand(true);
        app_label.add_css_class("notif-app-name");
        header.append(&app_label);

        let time_label = gtk::Label::new(Some(&time_ago(notif.timestamp)));
        time_label.add_css_class("notif-time");
        header.append(&time_label);

        let dismiss = gtk::Button::from_icon_name("window-close-symbolic");
        dismiss.add_css_class("flat");
        dismiss.add_css_class("notif-dismiss");
        dismiss.set_valign(gtk::Align::Center);
        let c = self.client.clone();
        let id = notif.id;
        dismiss.connect_clicked(move |_| {
            spawn_call(&c, "notifications.dismiss", serde_json::json!({"id": id}));
        });
        header.append(&dismiss);

        card.append(&header);

        // Summary (title)
        let summary = gtk::Label::new(Some(&notif.summary));
        summary.set_halign(gtk::Align::Start);
        summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
        summary.set_max_width_chars(40);
        summary.add_css_class("notif-summary");
        card.append(&summary);

        // Body
        if !notif.body.is_empty() {
            let body = gtk::Label::new(Some(&notif.body));
            body.set_halign(gtk::Align::Start);
            body.set_ellipsize(gtk::pango::EllipsizeMode::End);
            body.set_max_width_chars(45);
            body.set_lines(2);
            body.set_wrap(true);
            body.set_wrap_mode(gtk::pango::WrapMode::WordChar);
            body.add_css_class("notif-body");
            card.append(&body);
        }

        // Action buttons
        let visible_actions: Vec<&(String, String)> = notif
            .actions
            .iter()
            .filter(|(key, _)| key != "default")
            .collect();
        if !visible_actions.is_empty() {
            let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            actions_box.add_css_class("notif-actions");
            for (key, label) in &visible_actions {
                let action_btn = gtk::Button::with_label(label);
                action_btn.add_css_class("flat");
                action_btn.add_css_class("notif-action-btn");
                let c = self.client.clone();
                let nid = notif.id;
                let k = key.clone();
                action_btn.connect_clicked(move |_| {
                    spawn_call(
                        &c,
                        "notifications.invoke_action",
                        serde_json::json!({"id": nid, "action_key": k}),
                    );
                });
                actions_box.append(&action_btn);
            }
            card.append(&actions_box);
        }

        // Click card body → default action
        let has_default = notif.actions.iter().any(|(k, _)| k == "default");
        if has_default {
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            let c = self.client.clone();
            let id = notif.id;
            gesture.connect_pressed(move |g, _, _, _| {
                g.set_state(gtk::EventSequenceState::Claimed);
                spawn_call(
                    &c,
                    "notifications.invoke_action",
                    serde_json::json!({"id": id, "action_key": "default"}),
                );
            });
            card.add_controller(gesture);
        }

        card.upcast()
    }
}
