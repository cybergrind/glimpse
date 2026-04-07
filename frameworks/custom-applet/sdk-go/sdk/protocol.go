package sdk

type Icon struct {
	Type  string `json:"type"`
	Value string `json:"value"`
}

func IconName(value string) *Icon {
	return &Icon{Type: "name", Value: value}
}

func IconPath(value string) *Icon {
	return &Icon{Type: "path", Value: value}
}

type StatusItem struct {
	ID   string `json:"id,omitempty"`
	Icon *Icon  `json:"icon,omitempty"`
	Text string `json:"text,omitempty"`
}

type Hero struct {
	Title    string `json:"title"`
	Subtitle string `json:"subtitle"`
	Icon     *Icon  `json:"icon,omitempty"`
}
