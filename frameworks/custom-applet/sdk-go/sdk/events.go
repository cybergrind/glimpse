package sdk

import "encoding/json"

type InitEvent struct {
	Instance string
}

type CallbackEvent interface {
	CallbackID() string
	CallbackType() string
}

type ClickEvent struct {
	ID     string
	Button string
}

func (e ClickEvent) CallbackID() string   { return e.ID }
func (e ClickEvent) CallbackType() string { return "click" }

type ScrollEvent struct {
	ID     string
	DeltaY float64
}

func (e ScrollEvent) CallbackID() string   { return e.ID }
func (e ScrollEvent) CallbackType() string { return "scroll" }

type InputEvent struct {
	ID   string
	Text string
}

func (e InputEvent) CallbackID() string   { return e.ID }
func (e InputEvent) CallbackType() string { return "input" }

type ChangeEvent struct {
	ID    string
	Value any
}

func (e ChangeEvent) CallbackID() string   { return e.ID }
func (e ChangeEvent) CallbackType() string { return "change" }

type ToggleEvent struct {
	ID    string
	Value bool
}

func (e ToggleEvent) CallbackID() string   { return e.ID }
func (e ToggleEvent) CallbackType() string { return "toggle" }

type incomingMessage struct {
	Type string          `json:"type"`
	Data json.RawMessage `json:"data"`
}

type initPayload struct {
	Instance string `json:"instance"`
}

type callbackPayload struct {
	ID     string `json:"id"`
	Event  string `json:"event"`
	Button string `json:"button,omitempty"`
	DeltaY float64 `json:"delta_y,omitempty"`
	Text   string `json:"text,omitempty"`
	Value  any    `json:"value,omitempty"`
}

func parseInitEvent(data []byte) (InitEvent, error) {
	var payload initPayload
	if err := json.Unmarshal(data, &payload); err != nil {
		return InitEvent{}, err
	}
	return InitEvent{Instance: payload.Instance}, nil
}

func parseCallbackEvent(data []byte) (CallbackEvent, error) {
	var payload callbackPayload
	if err := json.Unmarshal(data, &payload); err != nil {
		return nil, err
	}
	switch payload.Event {
	case "click":
		return ClickEvent{ID: payload.ID, Button: payload.Button}, nil
	case "scroll":
		return ScrollEvent{ID: payload.ID, DeltaY: payload.DeltaY}, nil
	case "input":
		return InputEvent{ID: payload.ID, Text: payload.Text}, nil
	case "toggle":
		value, _ := payload.Value.(bool)
		return ToggleEvent{ID: payload.ID, Value: value}, nil
	default:
		return ChangeEvent{ID: payload.ID, Value: payload.Value}, nil
	}
}
