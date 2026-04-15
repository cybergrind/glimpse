package sdk

type Align string
type Orientation string

const (
	AlignFill     Align = "fill"
	AlignStart    Align = "start"
	AlignEnd      Align = "end"
	AlignCenter   Align = "center"
	AlignBaseline Align = "baseline"

	OrientationHorizontal Orientation = "horizontal"
	OrientationVertical   Orientation = "vertical"
)

type CommonProps struct {
	ID         string   `json:"id,omitempty"`
	Visible    *bool    `json:"visible,omitempty"`
	HExpand    *bool    `json:"hexpand,omitempty"`
	VExpand    *bool    `json:"vexpand,omitempty"`
	HAlign     Align    `json:"halign,omitempty"`
	VAlign     Align    `json:"valign,omitempty"`
	Tooltip    string   `json:"tooltip,omitempty"`
	CSSClasses []string `json:"css_classes,omitempty"`
}

type TreeNode struct {
	Type string `json:"type"`
	Data any    `json:"data"`
}

type Hero struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle"`
	Icon     *Icon  `json:"icon,omitempty"`
}

func NewHero(title string, subtitle string) TreeNode {
	return TreeNode{Type: "hero", Data: Hero{Title: title, Subtitle: subtitle}}
}

type IconWidget struct {
	CommonProps
	Icon      *Icon `json:"icon"`
	PixelSize *int  `json:"pixel_size,omitempty"`
}

func NewIcon(icon *Icon) TreeNode {
	return TreeNode{Type: "icon", Data: IconWidget{Icon: icon}}
}

type Progress struct {
	CommonProps
	Value    float64 `json:"value"`
	Max      float64 `json:"max"`
	ShowText bool    `json:"show_text,omitempty"`
	Text     string  `json:"text,omitempty"`
}

func NewProgress(value float64) TreeNode {
	return TreeNode{Type: "progress", Data: Progress{Value: value, Max: 1}}
}

type Label struct {
	CommonProps
	Text       string  `json:"text"`
	Wrap       bool    `json:"wrap,omitempty"`
	XAlign     *float32 `json:"xalign,omitempty"`
	Selectable bool    `json:"selectable,omitempty"`
}

func NewLabel(text string) TreeNode {
	return TreeNode{Type: "label", Data: Label{Text: text}}
}

type Image struct {
	CommonProps
	Icon      *Icon `json:"icon"`
	PixelSize *int  `json:"pixel_size,omitempty"`
}

func NewImage(icon *Icon) TreeNode {
	return TreeNode{Type: "image", Data: Image{Icon: icon}}
}

type Button struct {
	CommonProps
	Label string    `json:"label,omitempty"`
	Icon  *Icon     `json:"icon,omitempty"`
	Child *TreeNode `json:"child,omitempty"`
}

func NewButton(id string, label string) TreeNode {
	return TreeNode{Type: "button", Data: Button{CommonProps: CommonProps{ID: id}, Label: label}}
}

type Entry struct {
	CommonProps
	Text        string `json:"text"`
	Placeholder string `json:"placeholder,omitempty"`
}

func NewEntry(id string, text string) TreeNode {
	return TreeNode{Type: "entry", Data: Entry{CommonProps: CommonProps{ID: id}, Text: text}}
}

type Password struct {
	CommonProps
	Text        string `json:"text"`
	Placeholder string `json:"placeholder,omitempty"`
}

func NewPassword(id string) TreeNode {
	return TreeNode{Type: "password", Data: Password{CommonProps: CommonProps{ID: id}}}
}

type Switch struct {
	CommonProps
	Label  string `json:"label,omitempty"`
	Active bool   `json:"active"`
}

func NewSwitch(id string, active bool) TreeNode {
	return TreeNode{Type: "switch", Data: Switch{CommonProps: CommonProps{ID: id}, Active: active}}
}

type Scale struct {
	CommonProps
	Min         float64     `json:"min"`
	Max         float64     `json:"max"`
	Step        float64     `json:"step"`
	Value       float64     `json:"value"`
	Orientation Orientation `json:"orientation,omitempty"`
	DrawValue   bool        `json:"draw_value,omitempty"`
}

func NewScale(id string, value float64) TreeNode {
	return TreeNode{
		Type: "scale",
		Data: Scale{
			CommonProps: CommonProps{ID: id},
			Min:         0,
			Max:         1,
			Step:        0.1,
			Value:       value,
		},
	}
}

type Checkbox struct {
	CommonProps
	Label  string `json:"label,omitempty"`
	Active bool   `json:"active"`
}

func NewCheckbox(id string, active bool) TreeNode {
	return TreeNode{Type: "checkbox", Data: Checkbox{CommonProps: CommonProps{ID: id}, Active: active}}
}

type DropdownItem struct {
	ID    string `json:"id"`
	Label string `json:"label"`
}

type Dropdown struct {
	CommonProps
	Items    []DropdownItem `json:"items"`
	Selected *uint32        `json:"selected,omitempty"`
}

func NewDropdown(id string, items []DropdownItem) TreeNode {
	return TreeNode{Type: "dropdown", Data: Dropdown{CommonProps: CommonProps{ID: id}, Items: items}}
}

type Separator struct {
	CommonProps
	Orientation Orientation `json:"orientation,omitempty"`
}

func NewSeparator() TreeNode {
	return TreeNode{Type: "separator", Data: Separator{}}
}

type Scroll struct {
	CommonProps
	Child TreeNode `json:"child"`
}

func NewScroll(child TreeNode) TreeNode {
	return TreeNode{Type: "scroll", Data: Scroll{Child: child}}
}

type GridChild struct {
	Row    int      `json:"row"`
	Column int      `json:"column"`
	Width  int      `json:"width"`
	Height int      `json:"height"`
	Child  TreeNode `json:"child"`
}

type Grid struct {
	CommonProps
	Children      []GridChild `json:"children"`
	RowSpacing    int         `json:"row_spacing"`
	ColumnSpacing int         `json:"column_spacing"`
}

func NewGrid(children []GridChild) TreeNode {
	return TreeNode{Type: "grid", Data: Grid{Children: children}}
}

type Box struct {
	CommonProps
	Orientation Orientation `json:"orientation"`
	Spacing     int         `json:"spacing"`
	Children    []TreeNode  `json:"children"`
}

type Card struct {
	CommonProps
	Children []TreeNode `json:"children"`
}

func NewCard(children []TreeNode) TreeNode {
	return TreeNode{Type: "card", Data: Card{Children: children}}
}

type Section struct {
	CommonProps
	Title    string     `json:"title"`
	Subtitle string     `json:"subtitle,omitempty"`
	Children []TreeNode `json:"children"`
}

func NewSection(title string, children []TreeNode) TreeNode {
	return TreeNode{Type: "section", Data: Section{Title: title, Children: children}}
}

type Row struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle,omitempty"`
	Meta     string `json:"meta,omitempty"`
	Icon     *Icon  `json:"icon,omitempty"`
}

func NewRow(id string, title string) TreeNode {
	return TreeNode{Type: "row", Data: Row{CommonProps: CommonProps{ID: id}, Title: title}}
}

type DetailGridItem struct {
	Key   string `json:"key"`
	Value string `json:"value"`
}

type DetailGrid struct {
	CommonProps
	Rows []DetailGridItem `json:"rows"`
}

func NewDetailGrid(rows []DetailGridItem) TreeNode {
	return TreeNode{Type: "detail_grid", Data: DetailGrid{Rows: rows}}
}

type FooterAction struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle,omitempty"`
}

func NewFooterAction(id string, title string) TreeNode {
	return TreeNode{Type: "footer_action", Data: FooterAction{CommonProps: CommonProps{ID: id}, Title: title}}
}

type EmptyState struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle,omitempty"`
}

func NewEmptyState(title string) TreeNode {
	return TreeNode{Type: "empty_state", Data: EmptyState{Title: title}}
}

type Badge struct {
	CommonProps
	Label string `json:"label"`
}

func NewBadge(label string) TreeNode {
	return TreeNode{Type: "badge", Data: Badge{Label: label}}
}

type StatusDot struct {
	CommonProps
}

func NewStatusDot() TreeNode {
	return TreeNode{Type: "status_dot", Data: StatusDot{}}
}

func BoxVertical(children []TreeNode, spacing int) TreeNode {
	return TreeNode{
		Type: "box",
		Data: Box{
			Orientation: OrientationVertical,
			Spacing:     spacing,
			Children:    children,
		},
	}
}

func BoxHorizontal(children []TreeNode, spacing int) TreeNode {
	return TreeNode{
		Type: "box",
		Data: Box{
			Orientation: OrientationHorizontal,
			Spacing:     spacing,
			Children:    children,
		},
	}
}
