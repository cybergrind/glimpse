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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    Normal,
    Muted,
    Accent,
    Success,
    Warning,
    Danger,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<Variant>,
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
pub struct IconWidget {
    #[serde(flatten)]
    pub common: CommonProps,
    pub icon: Icon,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_size: Option<i32>,
}

impl IconWidget {
    pub fn new(icon: Icon) -> Self {
        Self {
            common: CommonProps::default(),
            icon,
            pixel_size: None,
        }
    }

    pub fn pixel_size(mut self, pixel_size: i32) -> Self {
        self.pixel_size = Some(pixel_size);
        self
    }
}

with_common!(IconWidget);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Progress {
    #[serde(flatten)]
    pub common: CommonProps,
    pub value: f64,
    pub max: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub show_text: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Progress {
    pub fn new(value: f64) -> Self {
        Self {
            common: CommonProps::default(),
            value,
            max: 1.0,
            show_text: false,
            text: None,
        }
    }

    pub fn max(mut self, max: f64) -> Self {
        self.max = max;
        self
    }

    pub fn show_text(mut self, show_text: bool) -> Self {
        self.show_text = show_text;
        self
    }

    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }
}

with_common!(Progress);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Card {
    #[serde(flatten)]
    pub common: CommonProps,
    pub children: Vec<TreeNode>,
}

impl Card {
    pub fn new(children: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            children,
        }
    }
}

with_common!(Card);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Section {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<Header>,
    pub body: Vec<TreeNode>,
}

impl Section {
    pub fn new(title: impl Into<String>, body: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            header: Some(Header::new(title)),
            body,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        if let Some(header) = self.header.take() {
            self.header = Some(header.subtitle(subtitle));
        }
        self
    }
}

with_common!(Section);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Header {
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub subtitle: String,
}

impl Header {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: String::new(),
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Collapsible {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<Header>,
    pub expanded: bool,
    pub body: Vec<TreeNode>,
}

impl Collapsible {
    pub fn new(title: impl Into<String>, expanded: bool, body: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            header: Some(Header::new(title)),
            expanded,
            body,
        }
    }
}

with_common!(Collapsible);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<Box<TreeNode>>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub clickable: bool,
}

impl Item {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            left: None,
            label: label.into(),
            right: None,
            clickable: false,
        }
    }

    pub fn clickable(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            left: None,
            label: label.into(),
            right: None,
            clickable: true,
        }
    }

    pub fn left(mut self, left: TreeNode) -> Self {
        self.left = Some(Box::new(left));
        self
    }

    pub fn right(mut self, right: TreeNode) -> Self {
        self.right = Some(Box::new(right));
        self
    }
}

with_common!(Item);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollapsibleItem {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<Box<TreeNode>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right: Option<Box<TreeNode>>,
    pub expanded: bool,
    pub body: Vec<TreeNode>,
}

impl CollapsibleItem {
    pub fn new(label: impl Into<String>, expanded: bool, body: Vec<TreeNode>) -> Self {
        Self {
            common: CommonProps::default(),
            left: None,
            label: label.into(),
            right: None,
            expanded,
            body,
        }
    }
}

with_common!(CollapsibleItem);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Meter {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub interactive: bool,
}

impl Meter {
    pub fn new(label: impl Into<String>, value: f64, max: f64) -> Self {
        Self {
            common: CommonProps::default(),
            icon: None,
            label: label.into(),
            value,
            min: 0.0,
            max,
            step: 0.01,
            text: None,
            interactive: false,
        }
    }
}

with_common!(Meter);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Copyable {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub value: String,
}

impl Copyable {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
            value: value.into(),
        }
    }
}

with_common!(Copyable);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToastAction {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Toast {
    #[serde(flatten)]
    pub common: CommonProps,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ToastAction>,
}

impl Toast {
    pub fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            icon: None,
            title: title.into(),
            message: message.into(),
            action: None,
        }
    }
}

with_common!(Toast);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    pub subtitle: String,
    pub meta: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Icon>,
}

impl Row {
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            common: CommonProps {
                id: Some(id.into()),
                ..CommonProps::default()
            },
            title: title.into(),
            subtitle: String::new(),
            meta: String::new(),
            icon: None,
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }

    pub fn meta(mut self, meta: impl Into<String>) -> Self {
        self.meta = meta.into();
        self
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
}

with_common!(Row);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DetailGridItem {
    pub key: String,
    pub value: String,
}

impl DetailGridItem {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetailGrid {
    #[serde(flatten)]
    pub common: CommonProps,
    pub rows: Vec<DetailGridItem>,
}

impl DetailGrid {
    pub fn new(rows: Vec<DetailGridItem>) -> Self {
        Self {
            common: CommonProps::default(),
            rows,
        }
    }
}

with_common!(DetailGrid);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmptyState {
    #[serde(flatten)]
    pub common: CommonProps,
    pub title: String,
    pub subtitle: String,
}

impl EmptyState {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            title: title.into(),
            subtitle: String::new(),
        }
    }

    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = subtitle.into();
        self
    }
}

with_common!(EmptyState);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Badge {
    #[serde(flatten)]
    pub common: CommonProps,
    pub label: String,
}

impl Badge {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            common: CommonProps::default(),
            label: label.into(),
        }
    }
}

with_common!(Badge);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatusDot {
    #[serde(flatten)]
    pub common: CommonProps,
}

impl StatusDot {
    pub fn new() -> Self {
        Self {
            common: CommonProps::default(),
        }
    }
}

with_common!(StatusDot);

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TreeNode {
    Hero(Hero),
    Card(Card),
    Section(Section),
    Collapsible(Collapsible),
    Item(Item),
    CollapsibleItem(CollapsibleItem),
    Meter(Meter),
    Copyable(Copyable),
    Toast(Toast),
    #[serde(rename = "action_row")]
    Row(Row),
    DetailGrid(DetailGrid),
    EmptyState(EmptyState),
    Badge(Badge),
    StatusDot(StatusDot),
    Box(BoxNode),
    Grid(Grid),
    Scroll(Scroll),
    Progress(Progress),
    Separator(Separator),
    Label(Label),
    Icon(IconWidget),
    Image(Image),
    Button(Button),
    Switch(Switch),
    Scale(Scale),
    Dropdown(Dropdown),
    Checkbox(Checkbox),
}

impl From<Hero> for TreeNode {
    fn from(value: Hero) -> Self {
        Self::Hero(value)
    }
}
impl From<Card> for TreeNode {
    fn from(value: Card) -> Self {
        Self::Card(value)
    }
}
impl From<Section> for TreeNode {
    fn from(value: Section) -> Self {
        Self::Section(value)
    }
}
impl From<Collapsible> for TreeNode {
    fn from(value: Collapsible) -> Self {
        Self::Collapsible(value)
    }
}
impl From<Item> for TreeNode {
    fn from(value: Item) -> Self {
        Self::Item(value)
    }
}
impl From<CollapsibleItem> for TreeNode {
    fn from(value: CollapsibleItem) -> Self {
        Self::CollapsibleItem(value)
    }
}
impl From<Meter> for TreeNode {
    fn from(value: Meter) -> Self {
        Self::Meter(value)
    }
}
impl From<Copyable> for TreeNode {
    fn from(value: Copyable) -> Self {
        Self::Copyable(value)
    }
}
impl From<Toast> for TreeNode {
    fn from(value: Toast) -> Self {
        Self::Toast(value)
    }
}
impl From<Row> for TreeNode {
    fn from(value: Row) -> Self {
        Self::Row(value)
    }
}
impl From<DetailGrid> for TreeNode {
    fn from(value: DetailGrid) -> Self {
        Self::DetailGrid(value)
    }
}
impl From<EmptyState> for TreeNode {
    fn from(value: EmptyState) -> Self {
        Self::EmptyState(value)
    }
}
impl From<Badge> for TreeNode {
    fn from(value: Badge) -> Self {
        Self::Badge(value)
    }
}
impl From<StatusDot> for TreeNode {
    fn from(value: StatusDot) -> Self {
        Self::StatusDot(value)
    }
}
impl From<BoxNode> for TreeNode {
    fn from(value: BoxNode) -> Self {
        Self::Box(value)
    }
}
impl From<Grid> for TreeNode {
    fn from(value: Grid) -> Self {
        Self::Grid(value)
    }
}
impl From<Scroll> for TreeNode {
    fn from(value: Scroll) -> Self {
        Self::Scroll(value)
    }
}
impl From<Progress> for TreeNode {
    fn from(value: Progress) -> Self {
        Self::Progress(value)
    }
}
impl From<Separator> for TreeNode {
    fn from(value: Separator) -> Self {
        Self::Separator(value)
    }
}
impl From<Label> for TreeNode {
    fn from(value: Label) -> Self {
        Self::Label(value)
    }
}
impl From<IconWidget> for TreeNode {
    fn from(value: IconWidget) -> Self {
        Self::Icon(value)
    }
}
impl From<Image> for TreeNode {
    fn from(value: Image) -> Self {
        Self::Image(value)
    }
}
impl From<Button> for TreeNode {
    fn from(value: Button) -> Self {
        Self::Button(value)
    }
}
impl From<Switch> for TreeNode {
    fn from(value: Switch) -> Self {
        Self::Switch(value)
    }
}
impl From<Scale> for TreeNode {
    fn from(value: Scale) -> Self {
        Self::Scale(value)
    }
}
impl From<Dropdown> for TreeNode {
    fn from(value: Dropdown) -> Self {
        Self::Dropdown(value)
    }
}
impl From<Checkbox> for TreeNode {
    fn from(value: Checkbox) -> Self {
        Self::Checkbox(value)
    }
}
