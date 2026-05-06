package sdk

import (
	"bytes"
	"encoding/json"
	"fmt"
)

type InitEvent struct {
	Instance string
	Options  map[string]any
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

type PopoverEvent struct {
	Open bool
}

func (e PopoverEvent) CallbackID() string {
	return "popover"
}

func (e PopoverEvent) CallbackType() string {
	if e.Open {
		return "open"
	}
	return "close"
}

type incomingMessage struct {
	Type string
	Data json.RawMessage
}

type initPayload struct {
	Instance string         `json:"instance"`
	Options  map[string]any `json:"options"`
}

type callbackPayload struct {
	ID     string  `json:"id"`
	Event  string  `json:"type"`
	Button string  `json:"button,omitempty"`
	DeltaY float64 `json:"delta_y,omitempty"`
	Text   string  `json:"text,omitempty"`
	Value  any     `json:"value,omitempty"`
	Active *bool   `json:"active,omitempty"`
}

func parseIncomingLine(line []byte) (incomingMessage, error) {
	line = bytes.TrimSpace(line)
	if len(line) == 0 {
		return incomingMessage{}, nil
	}
	command, payload, ok := bytes.Cut(line, []byte(" "))
	if !ok || len(bytes.TrimSpace(payload)) == 0 {
		return incomingMessage{}, fmt.Errorf("missing command payload")
	}
	return incomingMessage{Type: string(command), Data: bytes.TrimSpace(payload)}, nil
}

func parseInitEvent(data []byte) (InitEvent, error) {
	var payload initPayload
	if err := json.Unmarshal(data, &payload); err != nil {
		return InitEvent{}, err
	}
	return InitEvent{
		Instance: payload.Instance,
		Options:  payload.Options,
	}, nil
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
		value := false
		if payload.Active != nil {
			value = *payload.Active
		} else if parsed, ok := payload.Value.(bool); ok {
			value = parsed
		}
		return ToggleEvent{ID: payload.ID, Value: value}, nil
	case "open":
		return PopoverEvent{Open: true}, nil
	case "close":
		return PopoverEvent{Open: false}, nil
	default:
		return ChangeEvent{ID: payload.ID, Value: payload.Value}, nil
	}
}
