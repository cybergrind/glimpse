use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use relm4::gtk::{self, glib, prelude::*};

use super::protocol::{
    AlignValue, BadgeNode, BoxNode, ButtonNode, CallbackData, CardNode, CheckboxNode,
    CommonProps, DetailGridNode, DropdownNode, EmptyStateNode, EntryNode, FooterActionNode,
    GridNode, HeroNode, IconNode, IconSource, ImageNode, LabelNode, OrientationValue,
    ProgressNode, RowNode, ScaleNode, SectionNode, SeparatorNode, StatusDotNode, SwitchNode,
    TreeNode,
};

pub type CallbackSink = Rc<dyn Fn(CallbackData)>;

#[derive(Clone)]
pub struct RenderCatalog {
    callback: CallbackSink,
    focus_targets: Rc<RefCell<HashMap<String, gtk::Widget>>>,
}

impl Default for RenderCatalog {
    fn default() -> Self {
        Self {
            callback: Rc::new(|_| {}),
            focus_targets: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

impl RenderCatalog {
    pub fn with_callback(callback: CallbackSink) -> Self {
        Self {
            callback,
            focus_targets: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    pub fn focus_targets(&self) -> HashMap<String, gtk::Widget> {
        self.focus_targets.borrow().clone()
    }

    pub fn render(&self, node: &TreeNode) -> Result<gtk::Widget, RenderError> {
        match node {
            TreeNode::Hero(data) => self.render_hero(data),
            TreeNode::Card(data) => self.render_card(data),
            TreeNode::Section(data) => self.render_section(data),
            TreeNode::Row(data) => self.render_row(data),
            TreeNode::DetailGrid(data) => self.render_detail_grid(data),
            TreeNode::FooterAction(data) => self.render_footer_action(data),
            TreeNode::EmptyState(data) => self.render_empty_state(data),
            TreeNode::Badge(data) => self.render_badge(data),
            TreeNode::StatusDot(data) => self.render_status_dot(data),
            TreeNode::Box(data) => self.render_box(data),
            TreeNode::Grid(data) => self.render_grid(data),
            TreeNode::Scroll(data) => {
                let scrolled = gtk::ScrolledWindow::new();
                apply_common_props(&scrolled, &data.common);
                let child = self.render(&data.child)?;
                scrolled.set_child(Some(&child));
                Ok(scrolled.upcast())
            }
            TreeNode::Progress(data) => self.render_progress(data),
            TreeNode::Separator(data) => self.render_separator(data),
            TreeNode::Label(data) => self.render_label(data),
            TreeNode::Icon(data) => self.render_icon(data),
            TreeNode::Image(data) => self.render_image(data),
            TreeNode::Button(data) => self.render_button(data),
            TreeNode::Entry(data) => self.render_entry(data, true),
            TreeNode::Password(data) => self.render_entry(data, false),
            TreeNode::Switch(data) => self.render_switch(data),
            TreeNode::Scale(data) => self.render_scale(data),
            TreeNode::Dropdown(data) => self.render_dropdown(data),
            TreeNode::Checkbox(data) => self.render_checkbox(data),
        }
    }

    fn render_hero(&self, data: &HeroNode) -> Result<gtk::Widget, RenderError> {
        let hero_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hero_box.add_css_class("hero-row");
        apply_common_props(&hero_box, &data.common);
        if let Some(icon) = &data.icon {
            let image = gtk::Image::new();
            image.set_pixel_size(32);
            image.add_css_class("hero-row__media");
            apply_icon_to_image(&image, icon);
            hero_box.append(&image);
        }
        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text_box.set_valign(gtk::Align::Center);
        text_box.add_css_class("hero-row__content");
        let title = gtk::Label::new(Some(&data.title));
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.add_css_class("hero-row__title");
        text_box.append(&title);
        let subtitle = gtk::Label::new(Some(&data.subtitle));
        subtitle.set_halign(gtk::Align::Start);
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("hero-row__subtitle");
        subtitle.set_visible(!data.subtitle.is_empty());
        text_box.append(&subtitle);
        hero_box.append(&text_box);
        Ok(hero_box.upcast())
    }

    fn render_card(&self, data: &CardNode) -> Result<gtk::Widget, RenderError> {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("card-surface");
        apply_common_props(&card, &data.common);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("card-surface__body");
        for child in &data.children {
            body.append(&self.render(child)?);
        }
        card.append(&body);
        Ok(card.upcast())
    }

    fn render_section(&self, data: &SectionNode) -> Result<gtk::Widget, RenderError> {
        let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        section.add_css_class("section-block");
        apply_common_props(&section, &data.common);

        let header = gtk::Box::new(gtk::Orientation::Vertical, 0);
        header.add_css_class("section-block__header");

        let title = gtk::Label::new(Some(&data.title));
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.add_css_class("section-block__title");
        header.append(&title);

        if !data.subtitle.is_empty() {
            let subtitle = gtk::Label::new(Some(&data.subtitle));
            subtitle.set_halign(gtk::Align::Start);
            subtitle.set_xalign(0.0);
            subtitle.add_css_class("hero-row__subtitle");
            header.append(&subtitle);
        }

        section.append(&header);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.add_css_class("section-block__body");
        for child in &data.children {
            body.append(&self.render(child)?);
        }
        section.append(&body);
        Ok(section.upcast())
    }

    fn render_row(&self, data: &RowNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        root.add_css_class("action-row");
        apply_common_props(&root, &data.common);

        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.add_css_class("action-row__button");
        button.set_hexpand(true);

        let shell = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        shell.set_valign(gtk::Align::Center);
        shell.add_css_class("action-row__content-shell");

        let leading = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        leading.set_valign(gtk::Align::Center);
        leading.add_css_class("action-row__leading");
        if let Some(icon) = &data.icon {
            let image = gtk::Image::new();
            apply_icon_to_image(&image, icon);
            leading.append(&image);
        }
        shell.append(&leading);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
        content.set_hexpand(true);
        content.set_valign(gtk::Align::Center);
        content.add_css_class("action-row__content");

        let title = gtk::Label::new(Some(&data.title));
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("action-row__title");
        content.append(&title);

        let subtitle = gtk::Label::new(Some(&data.subtitle));
        subtitle.set_halign(gtk::Align::Start);
        subtitle.set_xalign(0.0);
        subtitle.add_css_class("action-row__subtitle");
        subtitle.set_visible(!data.subtitle.is_empty());
        content.append(&subtitle);
        shell.append(&content);

        let meta = gtk::Label::new(Some(&data.meta));
        meta.set_valign(gtk::Align::Center);
        meta.add_css_class("action-row__meta");
        meta.set_visible(!data.meta.is_empty());
        shell.append(&meta);

        button.set_child(Some(&shell));
        root.append(&button);

        let trailing = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        trailing.set_valign(gtk::Align::Center);
        trailing.add_css_class("action-row__trailing");
        root.append(&trailing);

        if let Some(id) = &data.common.id {
            let callback = self.callback.clone();
            let callback_id = id.clone();
            button.connect_clicked(move |_| {
                callback(CallbackData {
                    id: callback_id.clone(),
                    event: "click".into(),
                    button: Some("left".into()),
                    ..CallbackData::default()
                });
            });
            self.register_focus_target(id, &button);
        } else {
            button.set_sensitive(false);
        }

        Ok(root.upcast())
    }

    fn render_detail_grid(&self, data: &DetailGridNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("detail-grid");
        apply_common_props(&root, &data.common);

        for item in &data.rows {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.add_css_class("detail-grid__row");

            let key = gtk::Label::new(Some(&item.key));
            key.add_css_class("detail-grid__key");
            key.set_halign(gtk::Align::Start);
            key.set_xalign(0.0);
            key.set_hexpand(true);

            let value = gtk::Label::new(Some(&item.value));
            value.add_css_class("detail-grid__value");
            value.set_halign(gtk::Align::End);
            value.set_xalign(1.0);

            row.append(&key);
            row.append(&value);
            root.append(&row);
        }

        Ok(root.upcast())
    }

    fn render_footer_action(&self, data: &FooterActionNode) -> Result<gtk::Widget, RenderError> {
        let mut common = data.common.clone();
        common.css_classes.push("action-row--footer".into());
        common.css_classes.push("footer-action".into());
        self.render_row(&RowNode {
            common,
            title: data.title.clone(),
            subtitle: data.subtitle.clone(),
            meta: String::new(),
            icon: None,
        })
    }

    fn render_empty_state(&self, data: &EmptyStateNode) -> Result<gtk::Widget, RenderError> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.set_halign(gtk::Align::Center);
        root.add_css_class("empty-state");
        apply_common_props(&root, &data.common);

        let title = gtk::Label::new(Some(&data.title));
        title.set_halign(gtk::Align::Center);
        title.set_xalign(0.5);
        title.set_justify(gtk::Justification::Center);
        title.add_css_class("empty-state__title");
        root.append(&title);

        let subtitle = gtk::Label::new(Some(&data.subtitle));
        subtitle.set_halign(gtk::Align::Center);
        subtitle.set_xalign(0.5);
        subtitle.set_justify(gtk::Justification::Center);
        subtitle.add_css_class("empty-state__subtitle");
        subtitle.set_visible(!data.subtitle.is_empty());
        root.append(&subtitle);

        Ok(root.upcast())
    }

    fn render_badge(&self, data: &BadgeNode) -> Result<gtk::Widget, RenderError> {
        let label = gtk::Label::new(Some(&data.label));
        label.add_css_class("badge");
        apply_common_props(&label, &data.common);
        Ok(label.upcast())
    }

    fn render_status_dot(&self, data: &StatusDotNode) -> Result<gtk::Widget, RenderError> {
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("status-dot");
        apply_common_props(&dot, &data.common);
        Ok(dot.upcast())
    }

    fn render_box(&self, data: &BoxNode) -> Result<gtk::Widget, RenderError> {
        let widget = gtk::Box::new(to_orientation(data.orientation), data.spacing);
        apply_common_props(&widget, &data.common);
        for child in &data.children {
            widget.append(&self.render(child)?);
        }
        Ok(widget.upcast())
    }

    fn render_grid(&self, data: &GridNode) -> Result<gtk::Widget, RenderError> {
        let grid = gtk::Grid::new();
        apply_common_props(&grid, &data.common);
        grid.set_row_spacing(data.row_spacing as u32);
        grid.set_column_spacing(data.column_spacing as u32);
        for child in &data.children {
            let rendered = self.render(&child.child)?;
            grid.attach(
                &rendered,
                child.column,
                child.row,
                child.width,
                child.height,
            );
        }
        Ok(grid.upcast())
    }

    fn render_label(&self, data: &LabelNode) -> Result<gtk::Widget, RenderError> {
        let label = gtk::Label::new(Some(&data.text));
        apply_common_props(&label, &data.common);
        label.set_wrap(data.wrap);
        label.set_selectable(data.selectable);
        if let Some(xalign) = data.xalign {
            label.set_xalign(xalign);
        }
        Ok(label.upcast())
    }

    fn render_icon(&self, data: &IconNode) -> Result<gtk::Widget, RenderError> {
        let image = gtk::Image::new();
        image.add_css_class("exec-icon");
        apply_common_props(&image, &data.common);
        apply_icon_to_image(&image, &data.icon);
        if let Some(pixel_size) = data.pixel_size {
            image.set_pixel_size(pixel_size);
        }
        Ok(image.upcast())
    }

    fn render_image(&self, data: &ImageNode) -> Result<gtk::Widget, RenderError> {
        let image = gtk::Image::new();
        apply_common_props(&image, &data.common);
        apply_icon_to_image(&image, &data.icon);
        if let Some(pixel_size) = data.pixel_size {
            image.set_pixel_size(pixel_size);
        }
        Ok(image.upcast())
    }

    fn render_button(&self, data: &ButtonNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("button", &data.common)?;
        let button = gtk::Button::new();
        apply_common_props(&button, &data.common);
        button.add_css_class("flat");
        if let Some(child) = &data.child {
            button.set_child(Some(&self.render(child)?));
        } else {
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
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

        let callback = self.callback.clone();
        let callback_id = id.clone();
        button.connect_clicked(move |_| {
            callback(CallbackData {
                id: callback_id.clone(),
                event: "click".into(),
                button: Some("left".into()),
                ..CallbackData::default()
            });
        });
        self.register_focus_target(&id, &button);
        Ok(button.upcast())
    }

    fn render_entry(&self, data: &EntryNode, visible: bool) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("entry", &data.common)?;
        let entry = gtk::Entry::new();
        apply_common_props(&entry, &data.common);
        entry.set_text(&data.text);
        entry.set_visibility(visible);
        if let Some(placeholder) = &data.placeholder {
            entry.set_placeholder_text(Some(placeholder));
        }
        let callback = self.callback.clone();
        let debounce = Rc::new(RefCell::new(None));
        let callback_id = id.clone();
        entry.connect_changed(move |entry| {
            emit_callback(
                &callback,
                &debounce,
                CallbackData {
                    id: callback_id.clone(),
                    event: "input".into(),
                    text: Some(entry.text().to_string()),
                    ..CallbackData::default()
                },
            );
        });
        self.register_focus_target(&id, &entry);
        Ok(entry.upcast())
    }

    fn render_switch(&self, data: &SwitchNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("switch", &data.common)?;
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        apply_common_props(&row, &data.common);
        if let Some(label) = &data.label {
            let text = gtk::Label::new(Some(label));
            text.set_hexpand(true);
            text.set_halign(gtk::Align::Start);
            row.append(&text);
        }
        let switch = gtk::Switch::new();
        switch.set_active(data.active);
        let callback = self.callback.clone();
        let callback_id = id.clone();
        switch.connect_active_notify(move |switch| {
            callback(CallbackData {
                id: callback_id.clone(),
                event: "toggle".into(),
                value: Some(serde_json::Value::Bool(switch.is_active())),
                ..CallbackData::default()
            });
        });
        row.append(&switch);
        self.register_focus_target(&id, &switch);
        Ok(row.upcast())
    }

    fn render_scale(&self, data: &ScaleNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("scale", &data.common)?;
        let scale = gtk::Scale::with_range(
            to_orientation(data.orientation.unwrap_or(OrientationValue::Horizontal)),
            data.min,
            data.max,
            data.step,
        );
        apply_common_props(&scale, &data.common);
        scale.set_draw_value(data.draw_value);
        scale.set_value(data.value);
        let callback = self.callback.clone();
        let debounce = Rc::new(RefCell::new(None));
        let callback_id = id.clone();
        scale.connect_value_changed(move |scale| {
            emit_callback(
                &callback,
                &debounce,
                CallbackData {
                    id: callback_id.clone(),
                    event: "change".into(),
                    value: Some(serde_json::Value::from(scale.value())),
                    ..CallbackData::default()
                },
            );
        });
        self.register_focus_target(&id, &scale);
        Ok(scale.upcast())
    }

    fn render_progress(&self, data: &ProgressNode) -> Result<gtk::Widget, RenderError> {
        let progress = gtk::ProgressBar::new();
        progress.add_css_class("exec-progress");
        apply_common_props(&progress, &data.common);
        let fraction = if data.max <= 0.0 {
            0.0
        } else {
            (data.value / data.max).clamp(0.0, 1.0)
        };
        progress.set_fraction(fraction);
        if data.show_text {
            progress.set_show_text(true);
            progress.set_text(data.text.as_deref());
        } else if let Some(text) = &data.text {
            progress.set_show_text(true);
            progress.set_text(Some(text));
        }
        Ok(progress.upcast())
    }

    fn render_separator(&self, data: &SeparatorNode) -> Result<gtk::Widget, RenderError> {
        let separator = gtk::Separator::new(to_orientation(
            data.orientation.unwrap_or(OrientationValue::Horizontal),
        ));
        apply_common_props(&separator, &data.common);
        Ok(separator.upcast())
    }

    fn render_checkbox(&self, data: &CheckboxNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("checkbox", &data.common)?;
        let checkbox = if let Some(label) = &data.label {
            gtk::CheckButton::with_label(label)
        } else {
            gtk::CheckButton::new()
        };
        apply_common_props(&checkbox, &data.common);
        checkbox.set_active(data.active);
        let callback = self.callback.clone();
        let callback_id = id.clone();
        checkbox.connect_toggled(move |button| {
            callback(CallbackData {
                id: callback_id.clone(),
                event: "toggle".into(),
                value: Some(serde_json::Value::Bool(button.is_active())),
                ..CallbackData::default()
            });
        });
        self.register_focus_target(&id, &checkbox);
        Ok(checkbox.upcast())
    }

    fn render_dropdown(&self, data: &DropdownNode) -> Result<gtk::Widget, RenderError> {
        let id = require_interactive_id("dropdown", &data.common)?;
        let labels: Vec<&str> = data.items.iter().map(|item| item.label.as_str()).collect();
        let dropdown = gtk::DropDown::from_strings(&labels);
        apply_common_props(&dropdown, &data.common);
        if let Some(selected) = data.selected {
            dropdown.set_selected(selected);
        }
        let items = data.items.clone();
        let callback = self.callback.clone();
        let debounce = Rc::new(RefCell::new(None));
        let callback_id = id.clone();
        dropdown.connect_selected_notify(move |dropdown| {
            let index = dropdown.selected();
            let value = items
                .get(index as usize)
                .map(|item| serde_json::json!({"id": item.id, "label": item.label, "index": index}))
                .unwrap_or_else(|| serde_json::json!({"index": index}));
            emit_callback(
                &callback,
                &debounce,
                CallbackData {
                    id: callback_id.clone(),
                    event: "change".into(),
                    value: Some(value),
                    ..CallbackData::default()
                },
            );
        });
        self.register_focus_target(&id, &dropdown);
        Ok(dropdown.upcast())
    }

    fn register_focus_target(&self, id: &str, widget: &impl IsA<gtk::Widget>) {
        self.focus_targets
            .borrow_mut()
            .insert(id.to_owned(), widget.clone().upcast());
    }
}

#[derive(Debug)]
pub enum RenderError {
    MissingInteractiveId { widget_type: &'static str },
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::MissingInteractiveId { widget_type } => {
                write!(f, "{widget_type} widgets require an id for callbacks")
            }
        }
    }
}

pub fn apply_icon_to_image(image: &gtk::Image, icon: &IconSource) {
    match icon {
        IconSource::Name(name) => image.set_icon_name(Some(name)),
        IconSource::Path(path) => image.set_from_file(Some(path)),
    }
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
    for class_name in &props.css_classes {
        widget.add_css_class(class_name);
    }
}

fn require_interactive_id(
    widget_type: &'static str,
    props: &CommonProps,
) -> Result<String, RenderError> {
    props
        .id
        .clone()
        .ok_or(RenderError::MissingInteractiveId { widget_type })
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

fn to_orientation(value: OrientationValue) -> gtk::Orientation {
    match value {
        OrientationValue::Horizontal => gtk::Orientation::Horizontal,
        OrientationValue::Vertical => gtk::Orientation::Vertical,
    }
}

fn emit_callback(
    callback: &CallbackSink,
    debounce: &Rc<RefCell<Option<glib::SourceId>>>,
    data: CallbackData,
) {
    if let Some(delay_ms) = callback_debounce_delay_ms(&data.event) {
        if let Some(source_id) = debounce.borrow_mut().take() {
            source_id.remove();
        }
        let callback = callback.clone();
        let debounce = debounce.clone();
        let clear_handle = debounce.clone();
        *debounce.borrow_mut() = Some(glib::timeout_add_local_once(
            Duration::from_millis(delay_ms),
            move || {
                callback(data);
                clear_handle.borrow_mut().take();
            },
        ));
    } else {
        callback(data);
    }
}

fn callback_debounce_delay_ms(event: &str) -> Option<u64> {
    match event {
        "input" | "change" => Some(300),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{RenderCatalog, RenderError};
    use crate::applets::exec::protocol::TreeNode;

    #[test]
    fn renderer_rejects_unknown_widget_types() {
        let value = serde_json::json!({
            "type": "unknown_widget",
            "data": {}
        });

        let err = serde_json::from_value::<TreeNode>(value).expect_err("node should fail to parse");

        assert!(err.to_string().contains("unknown_widget"));
    }

    #[test]
    fn renderer_requires_ids_for_interactive_nodes() {
        let node = serde_json::from_value::<TreeNode>(serde_json::json!({
            "type": "button",
            "data": {
                "label": "Refresh"
            }
        }))
        .expect("button should parse");

        let err = RenderCatalog::default()
            .render(&node)
            .expect_err("interactive node should require an id");

        assert!(matches!(err, RenderError::MissingInteractiveId { .. }));
    }

    #[test]
    fn render_error_display_mentions_widget_type() {
        let err = RenderError::MissingInteractiveId {
            widget_type: "button",
        };

        assert_eq!(
            err.to_string(),
            "button widgets require an id for callbacks"
        );
    }

    #[test]
    fn callback_debounce_policy_delays_input_and_change() {
        assert_eq!(super::callback_debounce_delay_ms("input"), Some(300));
        assert_eq!(super::callback_debounce_delay_ms("change"), Some(300));
    }

    #[test]
    fn callback_debounce_policy_keeps_click_and_toggle_immediate() {
        assert_eq!(super::callback_debounce_delay_ms("click"), None);
        assert_eq!(super::callback_debounce_delay_ms("toggle"), None);
    }

    #[test]
    fn renderer_parses_label_and_entry_nodes() {
        let label = serde_json::from_value::<TreeNode>(serde_json::json!({
            "type": "label",
            "data": {
                "text": "Connected",
                "css_classes": ["dim-label"]
            }
        }))
        .expect("label should parse");
        assert!(matches!(label, TreeNode::Label(_)));

        let entry = serde_json::from_value::<TreeNode>(serde_json::json!({
            "type": "entry",
            "data": {
                "id": "version",
                "text": "v1"
            }
        }))
        .expect("entry should parse");
        assert!(matches!(entry, TreeNode::Entry(_)));
    }
}
