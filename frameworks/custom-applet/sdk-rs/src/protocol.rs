use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum Icon {
    Name { name: String },
    Path { path: String },
}

impl Icon {
    pub fn name(value: impl Into<String>) -> Self {
        Self::Name { name: value.into() }
    }

    pub fn path(value: impl Into<String>) -> Self {
        Self::Path { path: value.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct StatusItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub menu: Vec<StatusMenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusMenuItem {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl StatusMenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            visible: None,
            enabled: None,
        }
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }
}

impl StatusItem {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            ..Self::default()
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn menu(mut self, menu: Vec<StatusMenuItem>) -> Self {
        self.menu = menu;
        self
    }

    pub fn menu_item(mut self, item: StatusMenuItem) -> Self {
        self.menu.push(item);
        self
    }
}
