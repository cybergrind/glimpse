use std::rc::Rc;

use relm4::{
    WidgetTemplate,
    gtk::{self, prelude::*},
};

use crate::components::{
    action_row::{ActionRow, ActionRowInit},
    badge::BadgeView,
    card_surface::CardSurface,
    empty_state::EmptyStateView,
    hero::HeroView,
    key_value_grid::{KeyValueItem, static_key_value_grid},
    section_header::SectionHeader,
    status_dot::StatusDotView,
};

use super::protocol::{
    AlignValue, BadgeNode, BoxNode, ButtonNode, CardNode, CheckboxNode, CommonProps,
    DetailGridNode, DropdownNode, EmptyStateNode, EventKind, EventPayload, EventSource, GridNode,
    HeroNode, Icon, IconNode, ImageNode, LabelNode, OrientationValue, ProgressNode, RowNode,
    ScaleNode, ScrollNode, SectionNode, SeparatorNode, StatusDotNode, SwitchNode, TreeNode,
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
            TreeNode::Row(data) => self.render_row(data),
            TreeNode::DetailGrid(data) => Ok(self.render_detail_grid(data).upcast()),
            TreeNode::EmptyState(data) => Ok(self.render_empty_state(data).upcast()),
            TreeNode::Badge(data) => Ok(self.render_badge(data).upcast()),
            TreeNode::StatusDot(data) => Ok(self.render_status_dot(data).upcast()),
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
        apply_common_props(&root, &data.common);

        let header = SectionHeader::init(());
        header.title.set_label(&data.title);
        header.subtitle.set_label(&data.subtitle);
        header.subtitle.set_visible(!data.subtitle.is_empty());
        root.append(header.as_ref());

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("section-block__body");
        for child in &data.children {
            body.append(&self.render(child)?);
        }
        root.append(&body);
        Ok(root.upcast())
    }

    fn render_row(&self, data: &RowNode) -> Result<gtk::Widget, RenderError> {
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

    fn render_status_dot(&self, data: &StatusDotNode) -> gtk::Box {
        let dot = StatusDotView::init(());
        apply_common_props(dot.as_ref(), &data.common);
        dot.as_ref().clone()
    }

    fn render_box(&self, data: &BoxNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(to_orientation(data.orientation), data.spacing);
        apply_common_props(&root, &data.common);
        for child in &data.children {
            root.append(&self.render(child)?);
        }
        Ok(root.upcast())
    }

    fn render_grid(&self, data: &GridNode) -> Result<gtk::Widget, RenderError> {
        let grid = gtk::Grid::new();
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
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        apply_common_props(&scroll, &data.common);
        scroll.set_child(Some(&self.render(&data.child)?));
        Ok(scroll.upcast())
    }

    fn render_progress(&self, data: &ProgressNode) -> gtk::ProgressBar {
        let progress = gtk::ProgressBar::new();
        progress.add_css_class("exec-progress");
        apply_common_props(&progress, &data.common);
        let fraction = if data.max <= 0.0 {
            0.0
        } else {
            (data.value / data.max).clamp(0.0, 1.0)
        };
        progress.set_fraction(fraction);
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
        apply_common_props(&separator, &data.common);
        separator
    }

    fn render_label(&self, data: &LabelNode) -> gtk::Label {
        let label = gtk::Label::new(Some(&data.text));
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
        image.add_css_class("exec-icon");
        if let Some(pixel_size) = data.pixel_size {
            image.set_pixel_size(pixel_size);
        }
        apply_icon_to_image(&image, &data.icon);
        apply_common_props(&image, &data.common);
        image
    }

    fn render_image(&self, data: &ImageNode) -> gtk::Image {
        let image = gtk::Image::new();
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
}
