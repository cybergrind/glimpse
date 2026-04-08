use serde::Serialize;

use crate::protocol::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Fill,
    Start,
    End,
    Center,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct CommonProps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hexpand: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vexpand: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub halign: Option<Align>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valign: Option<Align>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub css_classes: Vec<String>,
}

macro_rules! with_common {
    ($name:ident) => {
        impl $name {
            pub fn id(mut self, id: impl Into<String>) -> Self {
                self.common.id = Some(id.into());
                self
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Label {
    #[serde(flatten)]
    pub common: CommonProps,
    pub text: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub wrap: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub xalign: Option<f32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub selectable: bool,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            text: text.into(),
            wrap: false,
            xalign: None,
            selectable: false,
        }
    }
}

with_common!(Label);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Image {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: Icon,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_size: Option<i32>,
}

impl Image {
    pub fn new(icon: Icon) -> Self {
        Self {
            common: CommonProps::default(),
            icon,
            pixel_size: None,
        }
    }
}

with_common!(Image);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Button {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<Box<TreeNode>>,
}

impl Button {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            label: None,
            icon: None,
            child: None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

with_common!(Button);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    #[serde(flatten)]
    pub common: CommonProps,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

impl Entry {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            text: String::new(),
            placeholder: None,
        }
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }
}

with_common!(Entry);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Password {
    #[serde(flatten)]
    pub common: CommonProps,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

impl Password {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            text: String::new(),
            placeholder: None,
        }
    }
}

with_common!(Password);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Switch {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
}

impl Switch {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            label: None,
            active: false,
        }
    }
}

with_common!(Switch);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scale {
    #[serde(flatten)]
    pub common: CommonProps,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub draw_value: bool,
}

impl Scale {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            min: 0.0,
            max: 1.0,
            step: 0.1,
            value: 0.0,
            orientation: None,
            draw_value: false,
        }
    }
}

with_common!(Scale);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Checkbox {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub active: bool,
}

impl Checkbox {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            label: None,
            active: false,
        }
    }
}

with_common!(Checkbox);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DropdownItem {
    pub id: String,
    pub label: String,
}

impl DropdownItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dropdown {
    #[serde(flatten)]
    pub common: CommonProps,
    pub items: Vec<DropdownItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<u32>,
}

impl Dropdown {
    pub fn new(id: impl Into<String>, items: Vec<DropdownItem>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            items,
            selected: None,
        }
    }
}

with_common!(Dropdown);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Separator {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,
}

impl Separator {
    pub fn new() -> Self {
        Self {
            common: CommonProps::default(),
            orientation: None,
        }
    }
}

with_common!(Separator);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scroll {
    #[serde(flatten)]
    pub common: CommonProps,
    pub child: Box<TreeNode>,
}

impl Scroll {
    pub fn new(child: TreeNode) -> Self {
        Self {
            common: CommonProps::default(),
            child: Box::new(child),
        }
    }
}

with_common!(Scroll);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GridChild {
    pub row: i32,
    pub column: i32,
    pub width: i32,
    pub height: i32,
    pub child: TreeNode,
}

impl GridChild {
    pub fn new(row: i32, column: i32, child: TreeNode) -> Self {
        Self {
            row,
            column,
            width: 1,
            height: 1,
            child,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Grid {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<GridChild>,
    pub row_spacing: i32,
    pub column_spacing: i32,
}

impl Grid {
    pub fn new(children: Vec<GridChild>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
            row_spacing: 0,
            column_spacing: 0,
        }
    }
}

with_common!(Grid);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BoxNode {
    #[serde(flatten)]
    pub common: CommonProps,
    pub orientation: Orientation,
    pub spacing: i32,
    pub children: Vec<TreeNode>,
}

impl BoxNode {
    pub fn vertical(children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            orientation: Orientation::Vertical,
            spacing: 0,
            children,
        }
    }

    pub fn horizontal(children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            orientation: Orientation::Horizontal,
            spacing: 0,
            children,
        }
    }

    pub fn spacing(mut self, spacing: i32) -> Self {
        self.spacing = spacing;
        self
    }
}

with_common!(BoxNode);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hero {
    pub title: String,
    pub subtitle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(flatten)]
    pub common: CommonProps,
}

impl Hero {
    pub fn new(title: impl Into<String>, subtitle: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: subtitle.into(),
            icon: None,
            common: CommonProps::default(),
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

with_common!(Hero);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TreeNode {
    Hero(Hero),
    Box(BoxNode),
    Grid(Grid),
    Scroll(Scroll),
    Separator(Separator),
    Label(Label),
    Image(Image),
    Button(Button),
    Entry(Entry),
    Password(Password),
    Switch(Switch),
    Scale(Scale),
    Dropdown(Dropdown),
    Checkbox(Checkbox),
}

impl From<Hero> for TreeNode {
    fn from(value: Hero) -> Self { Self::Hero(value) }
}
impl From<BoxNode> for TreeNode {
    fn from(value: BoxNode) -> Self { Self::Box(value) }
}
impl From<Grid> for TreeNode {
    fn from(value: Grid) -> Self { Self::Grid(value) }
}
impl From<Scroll> for TreeNode {
    fn from(value: Scroll) -> Self { Self::Scroll(value) }
}
impl From<Separator> for TreeNode {
    fn from(value: Separator) -> Self { Self::Separator(value) }
}
impl From<Label> for TreeNode {
    fn from(value: Label) -> Self { Self::Label(value) }
}
impl From<Image> for TreeNode {
    fn from(value: Image) -> Self { Self::Image(value) }
}
impl From<Button> for TreeNode {
    fn from(value: Button) -> Self { Self::Button(value) }
}
impl From<Entry> for TreeNode {
    fn from(value: Entry) -> Self { Self::Entry(value) }
}
impl From<Password> for TreeNode {
    fn from(value: Password) -> Self { Self::Password(value) }
}
impl From<Switch> for TreeNode {
    fn from(value: Switch) -> Self { Self::Switch(value) }
}
impl From<Scale> for TreeNode {
    fn from(value: Scale) -> Self { Self::Scale(value) }
}
impl From<Dropdown> for TreeNode {
    fn from(value: Dropdown) -> Self { Self::Dropdown(value) }
}
impl From<Checkbox> for TreeNode {
    fn from(value: Checkbox) -> Self { Self::Checkbox(value) }
}
