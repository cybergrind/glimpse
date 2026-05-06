use std::rc::Rc;

use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::components::{
    action_row::{ActionRow, ActionRowInit},
    badge::BadgeView,
    card_surface::CardSurface,
    collapsible_section::CollapsibleSectionView,
    copyable::CopyableView,
    empty_state::EmptyStateView,
    hero::HeroView,
    item::ItemView,
    key_value_grid::{KeyValueItem, static_key_value_grid},
    meter::MeterView,
    section_header::SectionHeader,
    status_dot::StatusDotView,
    toast::ToastView,
};

use super::protocol::{
    ActionMenuNode, ActionRowNode, AlignValue, BadgeNode, BoxNode, ButtonNode, CardNode,
    CheckboxNode, CollapsibleItemNode, CollapsibleNode, CommonProps, CopyableNode, DetailGridNode,
    DropdownNode, EmptyStateNode, EventKind, EventPayload, EventSource, GridNode, HeaderNode,
    HeroNode, Icon, IconNode, ImageNode, ItemNode, LabelNode, LayoutNode, MeterNode,
    OrientationValue, ProgressNode, ScaleNode, ScrollNode, SectionNode, SeparatorNode, SpinnerNode,
    StatusNode, SwitchNode, ToastNode, TreeNode,
};

pub type EventSink = Rc<dyn Fn(EventPayload)>;

#[derive(Clone)]
pub struct RenderCatalog {
    event: EventSink,
}

impl RenderCatalog {
    pub fn new(event: EventSink) -> Self {
        Self { event }
    }

    pub fn render(&self, node: &TreeNode) -> Result<gtk::Widget, RenderError> {
        match node {
            TreeNode::Hero(data) => self.render_hero(data),
            TreeNode::Card(data) => self.render_card(data),
            TreeNode::Section(data) => self.render_section(data),
            TreeNode::Collapsible(data) | TreeNode::CollapsibleSection(data) => {
                self.render_collapsible(data)
            }
            TreeNode::ActionMenu(data) => self.render_action_menu(data),
            TreeNode::Item(data) => self.render_item(data),
            TreeNode::CollapsibleItem(data) => self.render_collapsible_item(data),
            TreeNode::Meter(data) => self.render_meter(data),
            TreeNode::Copyable(data) => Ok(self.render_copyable(data).upcast()),
            TreeNode::Toast(data) => self.render_toast(data),
            TreeNode::Column(data) => {
                self.render_layout(data, gtk::Orientation::Vertical, "column")
            }
            TreeNode::Row(data) => self.render_layout(data, gtk::Orientation::Horizontal, "row"),
            TreeNode::ActionRow(data) => self.render_action_row(data),
            TreeNode::DetailGrid(data) => Ok(self.render_detail_grid(data).upcast()),
            TreeNode::EmptyState(data) => Ok(self.render_empty_state(data).upcast()),
            TreeNode::Badge(data) => Ok(self.render_badge(data).upcast()),
            TreeNode::Status(data) => Ok(self.render_status(data).upcast()),
            TreeNode::Spinner(data) => Ok(self.render_spinner(data).upcast()),
            TreeNode::Box(data) => self.render_box(data),
            TreeNode::Grid(data) => self.render_grid(data),
            TreeNode::Scroll(data) => self.render_scroll(data),
            TreeNode::Progress(data) => Ok(self.render_progress(data).upcast()),
            TreeNode::Separator(data) => Ok(self.render_separator(data).upcast()),
            TreeNode::Label(data) => Ok(self.render_label(data).upcast()),
            TreeNode::Icon(data) => Ok(self.render_icon(data).upcast()),
            TreeNode::Image(data) => Ok(self.render_image(data).upcast()),
            TreeNode::Button(data) => self.render_button(data),
            TreeNode::Switch(data) => self.render_switch(data),
            TreeNode::Checkbox(data) => self.render_checkbox(data),
            TreeNode::Scale(data) => self.render_scale(data),
            TreeNode::Dropdown(data) => self.render_dropdown(data),
        }
    }

    fn render_hero(&self, data: &HeroNode) -> Result<gtk::Widget, RenderError> {
        let hero = HeroView::init(());
        hero.title.set_label(&data.title);
        hero.subtitle.set_label(&data.subtitle);
        hero.subtitle.set_visible(!data.subtitle.is_empty());
        hero.icon.set_visible(data.icon.is_some());
        if let Some(icon) = &data.icon {
            apply_icon_to_image(&hero.icon, icon);
        }
        apply_common_props(hero.as_ref(), &data.common);
        Ok(hero.as_ref().clone().upcast())
    }

    fn render_card(&self, data: &CardNode) -> Result<gtk::Widget, RenderError> {
        let card = CardSurface::init(());
        apply_common_props(card.as_ref(), &data.common);
        for child in &data.children {
            card.body.append(&self.render(child)?);
        }
        Ok(card.as_ref().clone().upcast())
    }

    fn render_section(&self, data: &SectionNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("section-block");
        root.add_css_class("section");
        apply_common_props(&root, &data.common);

        if let Some(header_data) = section_header(data) {
            let header = SectionHeader::init(());
            header.title.set_label(&header_data.title);
            header.subtitle.set_label(&header_data.subtitle);
            header
                .subtitle
                .set_visible(!header_data.subtitle.is_empty());
            root.append(header.as_ref());
        }

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("section-block__body");
        body.add_css_class("section__body");
        for child in section_body(data) {
            body.append(&self.render(child)?);
        }
        root.append(&body);
        Ok(root.upcast())
    }

    fn render_collapsible(&self, data: &CollapsibleNode) -> Result<gtk::Widget, RenderError> {
        let section = CollapsibleSectionView::init(());
        section.as_ref().add_css_class("collapsible");
        let header = collapsible_header(data);
        section.title.set_label(
            header
                .as_ref()
                .map(|header| header.title.as_str())
                .unwrap_or_default(),
        );
        section.subtitle.set_label(
            header
                .as_ref()
                .map(|header| header.subtitle.as_str())
                .unwrap_or_default(),
        );
        section.subtitle.set_visible(
            header
                .as_ref()
                .is_some_and(|header| !header.subtitle.is_empty()),
        );
        section.content.set_visible(data.expanded);
        section.chevron.set_icon_name(Some(if data.expanded {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        }));
        for child in collapsible_body(data) {
            section.content.append(&self.render(child)?);
        }
        let content = section.content.clone();
        let chevron = section.chevron.clone();
        section.button.connect_clicked(move |_| {
            let expanded = !content.is_visible();
            content.set_visible(expanded);
            chevron.set_icon_name(Some(if expanded {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            }));
        });
        apply_common_props(section.as_ref(), &data.common);
        Ok(section.as_ref().clone().upcast())
    }

    fn render_action_row(&self, data: &ActionRowNode) -> Result<gtk::Widget, RenderError> {
        let row = ActionRow::init(ActionRowInit {
            title: data.title.clone(),
            subtitle: data.subtitle.clone(),
            meta: data.meta.clone(),
            icon: icon_name(&data.icon),
            visible: data.common.visible.unwrap_or(true),
            selectable: false,
        });
        apply_common_props(row.as_ref(), &data.common);
        row.button.set_sensitive(data.common.id.is_some());
        if let Some(icon) = &data.icon {
            apply_icon_to_image(&row.icon, icon);
            row.icon.set_visible(true);
        }

        if let Some(id) = &data.common.id {
            connect_click(&row.button, self.event.clone(), id.clone());
        }
        Ok(row.as_ref().clone().upcast())
    }

    fn render_layout(
        &self,
        data: &LayoutNode,
        orientation: gtk::Orientation,
        class_name: &'static str,
    ) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(orientation, data.spacing);
        root.add_css_class(class_name);
        apply_common_props(&root, &data.common);
        for child in &data.children {
            root.append(&self.render(child)?);
        }
        Ok(root.upcast())
    }

    fn render_action_menu(&self, data: &ActionMenuNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("action-menu");
        apply_common_props(&root, &data.common);

        if let Some(header) = &data.header {
            let header_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            header_box.add_css_class("action-menu__header");

            let title = gtk::Label::new(Some(header));
            title.set_halign(gtk::Align::Start);
            title.add_css_class("action-menu__title");
            header_box.append(&title);
            root.append(&header_box);
        }

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("action-menu__body");
        for item in &data.items {
            if !item.visible {
                continue;
            }
            let selectable = item.selectable.unwrap_or(item.checked.is_some());
            let checked = item.checked.unwrap_or(false);
            let row = ActionRow::init(ActionRowInit {
                title: item.label.clone(),
                subtitle: String::new(),
                meta: String::new(),
                icon: icon_name(&item.icon),
                visible: true,
                selectable,
            });
            if checked {
                row.as_ref().add_css_class("is-checked");
                row.as_ref().add_css_class("is-selected");
            }
            if let Some(icon) = &item.icon {
                apply_icon_to_image(&row.icon, icon);
                row.icon.set_visible(true);
            }
            connect_click(&row.button, self.event.clone(), item.id.clone());
            body.append(row.as_ref());
        }
        root.set_visible(data.items.iter().any(|item| item.visible));
        root.append(&body);
        Ok(root.upcast())
    }

    fn render_item(&self, data: &ItemNode) -> Result<gtk::Widget, RenderError> {
        let item = self.render_item_view(
            data.left.as_deref(),
            &data.label,
            data.right.as_deref(),
            None,
        )?;
        let button = item.button.clone();

        apply_common_props(&button, &data.common);

        if data.clickable {
            let id = require_id("item", &data.common)?;
            connect_click(&button, self.event.clone(), id);
        } else {
            button.add_css_class("item__button--static");
            button.set_focusable(false);
        }

        Ok(button.upcast())
    }

    fn render_collapsible_item(
        &self,
        data: &CollapsibleItemNode,
    ) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("collapsible-item");
        apply_common_props(&root, &data.common);

        let chevron = gtk::Image::new();
        chevron.add_css_class("collapsible-item__chevron");
        chevron.set_pixel_size(16);
        chevron.set_icon_name(Some(if data.expanded {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        }));

        let header = gtk::Button::new();
        header.add_css_class("flat");
        header.add_css_class("item");
        header.add_css_class("collapsible-item__button");
        let item = self.render_item_view(
            data.left.as_deref(),
            &data.label,
            data.right.as_deref(),
            Some(&chevron),
        )?;
        item.content.add_css_class("collapsible-item__content");
        item.left.add_css_class("collapsible-item__left");
        item.label.add_css_class("collapsible-item__label");
        item.right.add_css_class("collapsible-item__right");
        header.set_child(Some(&item.content));
        root.append(&header);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("collapsible-item__body");
        body.set_visible(data.expanded);
        for child in collapsible_item_body(data) {
            body.append(&self.render(child)?);
        }
        root.append(&body);

        let body_ref = body.clone();
        header.connect_clicked(move |_| {
            let expanded = !body_ref.is_visible();
            body_ref.set_visible(expanded);
            chevron.set_icon_name(Some(if expanded {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            }));
        });

        Ok(root.upcast())
    }

    fn render_item_view(
        &self,
        left: Option<&TreeNode>,
        label: &str,
        right: Option<&TreeNode>,
        chevron: Option<&gtk::Image>,
    ) -> Result<ItemView, RenderError> {
        let item = ItemView::init(());
        item.label.set_label(label);

        if let Some(left) = left {
            let child = self.render(left)?;
            constrain_slot_child(&child);
            item.left.append(&child);
            item.left.set_visible(true);
        }

        if let Some(right) = right {
            let child = self.render(right)?;
            constrain_slot_child(&child);
            item.right.append(&child);
            item.right.set_visible(true);
        }

        if let Some(chevron) = chevron {
            item.content.append(chevron);
        }

        Ok(item)
    }

    fn render_meter(&self, data: &MeterNode) -> Result<gtk::Widget, RenderError> {
        let meter = MeterView::init(());
        meter.label.set_label(&data.label);
        if let Some(icon) = &data.icon {
            apply_icon_to_image(&meter.icon, icon);
            meter.icon.set_visible(true);
        }
        if let Some(text) = &data.text {
            meter.value.set_label(text);
            meter.value.set_visible(true);
        }

        if data.interactive {
            let id = require_id("meter", &data.common)?;
            let (min, max) = meter_bounds(data.min, data.max, data.step);
            let scale = gtk::Scale::with_range(
                gtk::Orientation::Horizontal,
                min,
                max,
                data.step.max(f64::EPSILON),
            );
            scale.add_css_class("meter__scale");
            scale.add_css_class("scale");
            scale.set_draw_value(false);
            scale.set_value(data.value.clamp(min, max));
            let event = self.event.clone();
            scale.connect_value_changed(move |scale| {
                event(EventPayload {
                    id: id.clone(),
                    kind: EventKind::Change,
                    source: EventSource::Popover,
                    button: None,
                    active: None,
                    value: Some(serde_json::Value::from(scale.value())),
                    delta_y: None,
                });
            });
            meter.control.append(&scale);
        } else {
            let progress = gtk::ProgressBar::new();
            progress.add_css_class("meter__progress");
            progress.add_css_class("progress");
            progress.set_fraction(meter_fraction(data.value, data.min, data.max));
            meter.control.append(&progress);
        }

        apply_common_props(meter.as_ref(), &data.common);
        Ok(meter.as_ref().clone().upcast())
    }

    fn render_copyable(&self, data: &CopyableNode) -> gtk::Box {
        let copyable = CopyableView::init(());
        if !data.label.is_empty() {
            copyable.label.set_label(&data.label);
            copyable.label.set_visible(true);
        }
        copyable.value.set_label(&data.value);
        let copy_value = data.value.clone();
        copyable.button.connect_clicked(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&copy_value);
            }
        });
        apply_common_props(copyable.as_ref(), &data.common);
        copyable.as_ref().clone()
    }

    fn render_toast(&self, data: &ToastNode) -> Result<gtk::Widget, RenderError> {
        let toast = ToastView::init(());
        if let Some(icon) = &data.icon {
            apply_icon_to_image(&toast.icon, icon);
            toast.icon.set_visible(true);
        }
        toast.title.set_label(&data.title);
        toast.message.set_label(&data.message);
        toast.message.set_visible(!data.message.is_empty());

        if let Some(action) = &data.action {
            toast.action.set_label(&action.label);
            toast.action.set_visible(true);
            connect_click(&toast.action, self.event.clone(), action.id.clone());
        }

        apply_common_props(toast.as_ref(), &data.common);
        Ok(toast.as_ref().clone().upcast())
    }

    fn render_detail_grid(&self, data: &DetailGridNode) -> gtk::Box {
        let root = static_key_value_grid(
            data.rows
                .iter()
                .map(|item| KeyValueItem {
                    label: item.key.clone(),
                    value: item.value.clone(),
                    visible: true,
                })
                .collect(),
        );
        apply_common_props(&root, &data.common);
        root
    }

    fn render_empty_state(&self, data: &EmptyStateNode) -> gtk::Box {
        let empty = EmptyStateView::init(());
        empty.title.set_label(&data.title);
        empty.subtitle.set_label(&data.subtitle);
        empty.subtitle.set_visible(!data.subtitle.is_empty());
        apply_common_props(empty.as_ref(), &data.common);
        empty.as_ref().clone()
    }

    fn render_badge(&self, data: &BadgeNode) -> gtk::Label {
        let badge = BadgeView::init(());
        badge.set_label(&data.label);
        apply_common_props(badge.as_ref(), &data.common);
        badge.as_ref().clone()
    }

    fn render_status(&self, data: &StatusNode) -> gtk::Box {
        let dot = StatusDotView::init(());
        dot.add_css_class("status");
        apply_common_props(dot.as_ref(), &data.common);
        dot.as_ref().clone()
    }

    fn render_spinner(&self, data: &SpinnerNode) -> gtk::Spinner {
        let spinner = gtk::Spinner::new();
        spinner.add_css_class("spinner");
        spinner.set_spinning(data.spinning);
        apply_common_props(&spinner, &data.common);
        spinner
    }

    fn render_box(&self, data: &BoxNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(to_orientation(data.orientation), data.spacing);
        root.add_css_class(match data.orientation {
            OrientationValue::Horizontal => "row",
            OrientationValue::Vertical => "column",
        });
        apply_common_props(&root, &data.common);
        for child in &data.children {
            root.append(&self.render(child)?);
        }
        Ok(root.upcast())
    }

    fn render_grid(&self, data: &GridNode) -> Result<gtk::Widget, RenderError> {
        let grid = gtk::Grid::new();
        grid.add_css_class("grid");
        grid.set_row_spacing(data.row_spacing as u32);
        grid.set_column_spacing(data.column_spacing as u32);
        apply_common_props(&grid, &data.common);
        for child in &data.children {
            grid.attach(
                &self.render(&child.child)?,
                child.column,
                child.row,
                child.width,
                child.height,
            );
        }
        Ok(grid.upcast())
    }

    fn render_scroll(&self, data: &ScrollNode) -> Result<gtk::Widget, RenderError> {
        let scroll = gtk::ScrolledWindow::new();
        scroll.add_css_class("scroll");
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        apply_common_props(&scroll, &data.common);
        scroll.set_child(Some(&self.render(&data.child)?));
        Ok(scroll.upcast())
    }

    fn render_progress(&self, data: &ProgressNode) -> gtk::ProgressBar {
        let progress = gtk::ProgressBar::new();
        progress.add_css_class("progress");
        apply_common_props(&progress, &data.common);
        progress.set_fraction(progress_fraction(data.value, data.max));
        if data.show_text || data.text.is_some() {
            progress.set_show_text(true);
            progress.set_text(data.text.as_deref());
        }
        progress
    }

    fn render_separator(&self, data: &SeparatorNode) -> gtk::Separator {
        let separator = gtk::Separator::new(to_orientation(
            data.orientation.unwrap_or(OrientationValue::Horizontal),
        ));
        separator.add_css_class("separator");
        apply_common_props(&separator, &data.common);
        separator
    }

    fn render_label(&self, data: &LabelNode) -> gtk::Label {
        let label = gtk::Label::new(Some(&data.text));
        label.add_css_class("label");
        label.set_wrap(data.wrap);
        label.set_selectable(data.selectable);
        if let Some(xalign) = data.xalign {
            label.set_xalign(xalign);
        }
        apply_common_props(&label, &data.common);
        label
    }

    fn render_icon(&self, data: &IconNode) -> gtk::Image {
        let image = gtk::Image::new();
        image.add_css_class("icon");
        if let Some(pixel_size) = data.pixel_size {
            image.set_pixel_size(pixel_size);
        }
        apply_icon_to_image(&image, &data.icon);
        apply_common_props(&image, &data.common);
        image
    }

    fn render_image(&self, data: &ImageNode) -> gtk::Image {
        let image = gtk::Image::new();
        image.add_css_class("image");
        if let Some(pixel_size) = data.pixel_size {
            image.set_pixel_size(pixel_size);
        }
        apply_icon_to_image(&image, &data.icon);
        apply_common_props(&image, &data.common);
        image
    }

    fn render_button(&self, data: &ButtonNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("button", &data.common)?;
        let button = gtk::Button::new();
        button.add_css_class("button");
        button.add_css_class("flat");
        apply_common_props(&button, &data.common);
        if let Some(child) = &data.child {
            button.set_child(Some(&self.render(child)?));
        } else {
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            content.set_valign(gtk::Align::Center);
            if let Some(icon) = &data.icon {
                let image = gtk::Image::new();
                apply_icon_to_image(&image, icon);
                content.append(&image);
            }
            if let Some(label) = &data.label {
                content.append(&gtk::Label::new(Some(label)));
            }
            button.set_child(Some(&content));
        }
        connect_click(&button, self.event.clone(), id);
        Ok(button.upcast())
    }

    fn render_switch(&self, data: &SwitchNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("switch", &data.common)?;
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("switch");
        apply_common_props(&row, &data.common);
        if let Some(label) = &data.label {
            let text = gtk::Label::new(Some(label));
            text.set_xalign(0.0);
            text.set_hexpand(true);
            row.append(&text);
        }
        let switch = gtk::Switch::new();
        switch.set_active(data.active);
        let event = self.event.clone();
        switch.connect_state_set(move |_, active| {
            event(EventPayload {
                id: id.clone(),
                kind: EventKind::Toggle,
                source: EventSource::Popover,
                button: None,
                active: Some(active),
                value: None,
                delta_y: None,
            });
            gtk::glib::Propagation::Proceed
        });
        row.append(&switch);
        Ok(row.upcast())
    }

    fn render_checkbox(&self, data: &CheckboxNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("checkbox", &data.common)?;
        let checkbox = if let Some(label) = &data.label {
            gtk::CheckButton::with_label(label)
        } else {
            gtk::CheckButton::new()
        };
        checkbox.add_css_class("checkbox");
        checkbox.set_active(data.active);
        apply_common_props(&checkbox, &data.common);
        let event = self.event.clone();
        checkbox.connect_toggled(move |button| {
            event(EventPayload {
                id: id.clone(),
                kind: EventKind::Toggle,
                source: EventSource::Popover,
                button: None,
                active: Some(button.is_active()),
                value: None,
                delta_y: None,
            });
        });
        Ok(checkbox.upcast())
    }

    fn render_scale(&self, data: &ScaleNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("scale", &data.common)?;
        let scale = gtk::Scale::with_range(
            to_orientation(data.orientation.unwrap_or(OrientationValue::Horizontal)),
            data.min,
            data.max,
            data.step,
        );
        scale.add_css_class("scale");
        scale.set_value(data.value);
        scale.set_draw_value(data.draw_value);
        apply_common_props(&scale, &data.common);
        let event = self.event.clone();
        scale.connect_value_changed(move |scale| {
            event(EventPayload {
                id: id.clone(),
                kind: EventKind::Change,
                source: EventSource::Popover,
                button: None,
                active: None,
                value: Some(serde_json::Value::from(scale.value())),
                delta_y: None,
            });
        });
        Ok(scale.upcast())
    }

    fn render_dropdown(&self, data: &DropdownNode) -> Result<gtk::Widget, RenderError> {
        let id = require_id("dropdown", &data.common)?;
        let labels: Vec<&str> = data.items.iter().map(|item| item.label.as_str()).collect();
        let dropdown = gtk::DropDown::from_strings(&labels);
        dropdown.add_css_class("dropdown");
        if let Some(selected) = data.selected {
            dropdown.set_selected(selected);
        }
        apply_common_props(&dropdown, &data.common);
        let items = data.items.clone();
        let event = self.event.clone();
        dropdown.connect_selected_notify(move |dropdown| {
            let index = dropdown.selected();
            let value = items
                .get(index as usize)
                .map(|item| serde_json::json!({"id": item.id, "label": item.label, "index": index}))
                .unwrap_or_else(|| serde_json::json!({"index": index}));
            event(EventPayload {
                id: id.clone(),
                kind: EventKind::Change,
                source: EventSource::Popover,
                button: None,
                active: None,
                value: Some(value),
                delta_y: None,
            });
        });
        Ok(dropdown.upcast())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    MissingId { widget_type: &'static str },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingId { widget_type } => write!(f, "{widget_type} requires a string id"),
        }
    }
}

impl std::error::Error for RenderError {}

pub fn apply_icon_to_image(image: &gtk::Image, icon: &Icon) {
    match icon {
        Icon::Name { name } => image.set_icon_name(Some(name)),
        Icon::Path { path } => image.set_from_file(Some(path)),
    }
}

fn icon_name(icon: &Option<Icon>) -> Option<String> {
    match icon {
        Some(Icon::Name { name }) => Some(name.clone()),
        Some(Icon::Path { .. }) | None => None,
    }
}

fn section_header(data: &SectionNode) -> Option<HeaderNode> {
    data.header.clone().or_else(|| {
        data.title.as_ref().map(|title| HeaderNode {
            title: title.clone(),
            subtitle: data.subtitle.clone(),
        })
    })
}

fn section_body(data: &SectionNode) -> &[TreeNode] {
    if data.body.is_empty() {
        &data.children
    } else {
        &data.body
    }
}

fn collapsible_header(data: &CollapsibleNode) -> Option<HeaderNode> {
    data.header.clone().or_else(|| {
        data.title.as_ref().map(|title| HeaderNode {
            title: title.clone(),
            subtitle: data.subtitle.clone(),
        })
    })
}

fn collapsible_body(data: &CollapsibleNode) -> &[TreeNode] {
    if data.body.is_empty() {
        &data.children
    } else {
        &data.body
    }
}

fn collapsible_item_body(data: &CollapsibleItemNode) -> &[TreeNode] {
    if data.body.is_empty() {
        &data.children
    } else {
        &data.body
    }
}

fn progress_fraction(value: f64, max: f64) -> f64 {
    if max <= 0.0 {
        0.0
    } else {
        (value / max).clamp(0.0, 1.0)
    }
}

fn constrain_slot_child(widget: &impl IsA<gtk::Widget>) {
    widget.set_halign(gtk::Align::Center);
    widget.set_valign(gtk::Align::Center);
    widget.set_hexpand(false);
    widget.set_vexpand(false);
}

fn meter_bounds(min: f64, max: f64, step: f64) -> (f64, f64) {
    if max > min {
        (min, max)
    } else {
        (min, min + step.max(f64::EPSILON))
    }
}

fn meter_fraction(value: f64, min: f64, max: f64) -> f64 {
    if max <= min {
        0.0
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
}

fn connect_click(button: &gtk::Button, event: EventSink, id: String) {
    button.connect_clicked(move |_| {
        event(EventPayload {
            id: id.clone(),
            kind: EventKind::Click,
            source: EventSource::Popover,
            button: Some(super::protocol::MouseButton::Left),
            active: None,
            value: None,
            delta_y: None,
        });
    });
}

fn require_id(widget_type: &'static str, props: &CommonProps) -> Result<String, RenderError> {
    props
        .id
        .clone()
        .filter(|id| !id.is_empty())
        .ok_or(RenderError::MissingId { widget_type })
}

fn apply_common_props(widget: &impl IsA<gtk::Widget>, props: &CommonProps) {
    if let Some(visible) = props.visible {
        widget.set_visible(visible);
    }
    if let Some(hexpand) = props.hexpand {
        widget.set_hexpand(hexpand);
    }
    if let Some(vexpand) = props.vexpand {
        widget.set_vexpand(vexpand);
    }
    if let Some(halign) = props.halign {
        widget.set_halign(to_align(halign));
    }
    if let Some(valign) = props.valign {
        widget.set_valign(to_align(valign));
    }
    if let Some(tooltip) = &props.tooltip {
        widget.set_tooltip_text(Some(tooltip));
    }
    if let Some(class_name) = props.variant.and_then(|variant| variant.class_name()) {
        widget.add_css_class(class_name);
    }
}

fn to_orientation(value: OrientationValue) -> gtk::Orientation {
    match value {
        OrientationValue::Horizontal => gtk::Orientation::Horizontal,
        OrientationValue::Vertical => gtk::Orientation::Vertical,
    }
}

fn to_align(value: AlignValue) -> gtk::Align {
    match value {
        AlignValue::Fill => gtk::Align::Fill,
        AlignValue::Start => gtk::Align::Start,
        AlignValue::End => gtk::Align::End,
        AlignValue::Center => gtk::Align::Center,
        AlignValue::Baseline => gtk::Align::Baseline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::test_support::gtk_available_on_this_thread;

    #[test]
    fn buttons_require_ids_for_events() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let result = renderer.render(&TreeNode::Button(ButtonNode {
            common: CommonProps::default(),
            label: Some("Run".into()),
            icon: None,
            child: None,
        }));

        assert_eq!(
            result.err(),
            Some(RenderError::MissingId {
                widget_type: "button"
            })
        );
    }

    #[test]
    fn clickable_items_require_ids_for_events() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let result = renderer.render(&TreeNode::Item(ItemNode {
            common: CommonProps::default(),
            left: None,
            label: "Open".into(),
            right: None,
            clickable: true,
        }));

        assert_eq!(
            result.err(),
            Some(RenderError::MissingId {
                widget_type: "item"
            })
        );
    }

    #[test]
    fn interactive_meters_require_ids_for_events() {
        if !gtk_available_on_this_thread() {
            return;
        }

        let renderer = RenderCatalog::new(Rc::new(|_| {}));
        let result = renderer.render(&TreeNode::Meter(MeterNode {
            common: CommonProps::default(),
            icon: None,
            label: "Volume".into(),
            value: 0.5,
            min: 0.0,
            max: 1.0,
            step: 0.01,
            text: None,
            interactive: true,
        }));

        assert_eq!(
            result.err(),
            Some(RenderError::MissingId {
                widget_type: "meter"
            })
        );
    }
}
