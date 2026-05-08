package sdk

import "encoding/json"

type Align string
type Orientation string
type Variant string

const (
	AlignFill     Align = "fill"
	AlignStart    Align = "start"
	AlignEnd      Align = "end"
	AlignCenter   Align = "center"
	AlignBaseline Align = "baseline"

	OrientationHorizontal Orientation = "horizontal"
	OrientationVertical   Orientation = "vertical"

	VariantNormal  Variant = "normal"
	VariantMuted   Variant = "muted"
	VariantAccent  Variant = "accent"
	VariantSuccess Variant = "success"
	VariantWarning Variant = "warning"
	VariantDanger  Variant = "danger"
)

type CommonProps struct {
	ID      string  `json:"id,omitempty"`
	Visible *bool   `json:"visible,omitempty"`
	HExpand *bool   `json:"hexpand,omitempty"`
	VExpand *bool   `json:"vexpand,omitempty"`
	HAlign  Align   `json:"halign,omitempty"`
	VAlign  Align   `json:"valign,omitempty"`
	Tooltip string  `json:"tooltip,omitempty"`
	Variant Variant `json:"variant,omitempty"`
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
	Text       string   `json:"text"`
	Wrap       bool     `json:"wrap,omitempty"`
	XAlign     *float32 `json:"xalign,omitempty"`
	Selectable bool     `json:"selectable,omitempty"`
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

type Header struct {
	Title    string `json:"title"`
	Subtitle string `json:"subtitle,omitempty"`
}

type Section struct {
	CommonProps
	Title    string     `json:"-"`
	Subtitle string     `json:"-"`
	Children []TreeNode `json:"-"`
	Header   *Header    `json:"header,omitempty"`
	Body     []TreeNode `json:"body,omitempty"`
}

func NewSection(title string, children []TreeNode) TreeNode {
	return TreeNode{
		Type: "section",
		Data: Section{
			Title:    title,
			Children: children,
			Header:   &Header{Title: title},
			Body:     children,
		},
	}
}

func (s Section) MarshalJSON() ([]byte, error) {
	type alias Section
	value := alias(s)
	if value.Header == nil && (value.Title != "" || value.Subtitle != "") {
		value.Header = &Header{Title: value.Title, Subtitle: value.Subtitle}
	}
	if value.Body == nil && value.Children != nil {
		value.Body = value.Children
	}
	return marshalAlias(value)
}

type Collapsible struct {
	CommonProps
	Title    string     `json:"-"`
	Subtitle string     `json:"-"`
	Children []TreeNode `json:"-"`
	Header   *Header    `json:"header,omitempty"`
	Expanded bool       `json:"expanded"`
	Body     []TreeNode `json:"body,omitempty"`
}

func NewCollapsible(title string, expanded bool, children []TreeNode) TreeNode {
	return TreeNode{
		Type: "collapsible",
		Data: Collapsible{
			Title:    title,
			Children: children,
			Header:   &Header{Title: title},
			Expanded: expanded,
			Body:     children,
		},
	}
}

func NewCollapsibleSection(title string, expanded bool, children []TreeNode) TreeNode {
	return NewCollapsible(title, expanded, children)
}

func (c Collapsible) MarshalJSON() ([]byte, error) {
	type alias Collapsible
	value := alias(c)
	if value.Header == nil && (value.Title != "" || value.Subtitle != "") {
		value.Header = &Header{Title: value.Title, Subtitle: value.Subtitle}
	}
	if value.Body == nil && value.Children != nil {
		value.Body = value.Children
	}
	return marshalAlias(value)
}

type Item struct {
	CommonProps
	Left      *TreeNode  `json:"left,omitempty"`
	Label     string     `json:"label,omitempty"`
	Right     *TreeNode  `json:"right,omitempty"`
	Clickable bool       `json:"clickable,omitempty"`
	Menu      []MenuItem `json:"menu,omitempty"`
}

func NewItem(label string) TreeNode {
	return TreeNode{Type: "item", Data: Item{Label: label}}
}

func NewClickableItem(id string, label string) TreeNode {
	return TreeNode{
		Type: "item",
		Data: Item{
			CommonProps: CommonProps{ID: id},
			Label:       label,
			Clickable:   true,
		},
	}
}

type CollapsibleItem struct {
	CommonProps
	Left     *TreeNode  `json:"left,omitempty"`
	Label    string     `json:"label,omitempty"`
	Right    *TreeNode  `json:"right,omitempty"`
	Expanded bool       `json:"expanded,omitempty"`
	Body     []TreeNode `json:"body"`
}

func NewCollapsibleItem(label string, expanded bool, children []TreeNode) TreeNode {
	return TreeNode{
		Type: "collapsible_item",
		Data: CollapsibleItem{
			Label:    label,
			Expanded: expanded,
			Body:     children,
		},
	}
}

type Meter struct {
	CommonProps
	Icon        *Icon   `json:"icon,omitempty"`
	Label       string  `json:"label,omitempty"`
	Value       float64 `json:"value"`
	Min         float64 `json:"min,omitempty"`
	Max         float64 `json:"max"`
	Step        float64 `json:"step,omitempty"`
	Text        string  `json:"text,omitempty"`
	Interactive bool    `json:"interactive,omitempty"`
}

func NewMeter(label string, value float64, max float64) TreeNode {
	return TreeNode{
		Type: "meter",
		Data: Meter{
			Label: label,
			Value: value,
			Max:   max,
		},
	}
}

type Copyable struct {
	CommonProps
	Label string `json:"label,omitempty"`
	Value string `json:"value"`
}

func NewCopyable(label string, value string) TreeNode {
	return TreeNode{Type: "copyable", Data: Copyable{Label: label, Value: value}}
}

type ToastAction struct {
	ID    string `json:"id"`
	Label string `json:"label"`
}

type Toast struct {
	CommonProps
	Icon    *Icon        `json:"icon,omitempty"`
	Title   string       `json:"title"`
	Message string       `json:"message,omitempty"`
	Action  *ToastAction `json:"action,omitempty"`
}

func NewToast(title string, message string) TreeNode {
	return TreeNode{Type: "toast", Data: Toast{Title: title, Message: message}}
}

type ActionMenuItem struct {
	ID         string `json:"id"`
	Label      string `json:"label"`
	Icon       *Icon  `json:"icon,omitempty"`
	Visible    bool   `json:"visible"`
	Checked    *bool  `json:"checked,omitempty"`
	Selectable *bool  `json:"selectable,omitempty"`
}

type ActionMenu struct {
	CommonProps
	Header string           `json:"header,omitempty"`
	Items  []ActionMenuItem `json:"items"`
}

func NewActionMenu(header string, items []ActionMenuItem) TreeNode {
	return TreeNode{
		Type: "action_menu",
		Data: ActionMenu{
			Header: header,
			Items:  items,
		},
	}
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

type ActionRow struct {
	CommonProps
	Title    string `json:"title"`
	Subtitle string `json:"subtitle,omitempty"`
	Meta     string `json:"meta,omitempty"`
	Icon     *Icon  `json:"icon,omitempty"`
}

func NewActionRow(id string, title string) TreeNode {
	return TreeNode{Type: "action_row", Data: ActionRow{CommonProps: CommonProps{ID: id}, Title: title}}
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

type Status struct {
	CommonProps
}

func NewStatus() TreeNode {
	return TreeNode{Type: "status", Data: Status{}}
}

type Spinner struct {
	CommonProps
	Spinning bool `json:"spinning"`
}

func NewSpinner() TreeNode {
	return TreeNode{Type: "spinner", Data: Spinner{Spinning: true}}
}

type Layout struct {
	CommonProps
	Spacing  int        `json:"spacing"`
	Children []TreeNode `json:"children"`
}

func NewColumn(children []TreeNode, spacing int) TreeNode {
	return TreeNode{
		Type: "column",
		Data: Layout{
			Spacing:  spacing,
			Children: children,
		},
	}
}

func NewRow(children []TreeNode, spacing int) TreeNode {
	return TreeNode{
		Type: "row",
		Data: Layout{
			Spacing:  spacing,
			Children: children,
		},
	}
}

func BoxVertical(children []TreeNode, spacing int) TreeNode {
	return TreeNode{
		Type: "column",
		Data: Box{
			Orientation: OrientationVertical,
			Spacing:     spacing,
			Children:    children,
		},
	}
}

func BoxHorizontal(children []TreeNode, spacing int) TreeNode {
	return TreeNode{
		Type: "row",
		Data: Box{
			Orientation: OrientationHorizontal,
			Spacing:     spacing,
			Children:    children,
		},
	}
}

func marshalAlias(value any) ([]byte, error) {
	encoded, err := json.Marshal(value)
	if err != nil {
		return nil, err
	}
	var payload map[string]any
	if err := json.Unmarshal(encoded, &payload); err != nil {
		return nil, err
	}
	for key, value := range payload {
		if value == nil {
			delete(payload, key)
		}
	}
	return json.Marshal(payload)
}
