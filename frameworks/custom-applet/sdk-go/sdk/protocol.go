package sdk

type Icon struct {
	Name string `json:"name,omitempty"`
	Path string `json:"path,omitempty"`
}

func IconName(value string) *Icon {
	return &Icon{Name: value}
}

func IconPath(value string) *Icon {
	return &Icon{Path: value}
}

type StatusItem struct {
	ID      string           `json:"id,omitempty"`
	Icon    *Icon            `json:"icon,omitempty"`
	Label   string           `json:"label,omitempty"`
	Tooltip string           `json:"tooltip,omitempty"`
	Menu    []StatusMenuItem `json:"menu,omitempty"`
}

type StatusMenuItem struct {
	ID      string `json:"id"`
	Label   string `json:"label"`
	Visible *bool  `json:"visible,omitempty"`
	Enabled *bool  `json:"enabled,omitempty"`
}
