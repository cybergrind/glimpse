package sdk

import (
	"bytes"
	"context"
	"encoding/json"
	"testing"
)

type demoState struct {
	Version string
	Clicks  int
}

type demoApplet struct {
	BaseApplet[demoState]
}

func newDemoApplet() *demoApplet {
	return &demoApplet{
		BaseApplet: NewBaseApplet(demoState{Version: "v1"}),
	}
}

func (a *demoApplet) OnStart(context.Context) error { return nil }
func (a *demoApplet) OnInit(context.Context, InitEvent) error { return nil }

func (a *demoApplet) OnCallback(_ context.Context, event CallbackEvent) error {
	switch e := event.(type) {
	case InputEvent:
		if e.ID == "version" {
			a.SetState(func(state *demoState) {
				state.Version = e.Text
			})
		}
	case ClickEvent:
		if e.ID == "submit" {
			a.SetState(func(state *demoState) {
				state.Clicks++
			})
		}
	}
	return nil
}

func (a *demoApplet) Render(context.Context) (RenderResult, error) {
	return RenderResult{
		Status: []StatusItem{
			{ID: "demo", Icon: IconName("demo-symbolic"), Text: a.State().Version},
		},
		Hero: &Hero{Title: "Demo", Subtitle: a.State().Version},
		Tree: ptr(BoxVertical([]TreeNode{
			NewLabel(a.State().Version),
			NewButton("submit", "Submit"),
		}, 0)),
	}, nil
}

func TestParseCallbackEventReturnsTypedInputVariant(t *testing.T) {
	event, err := parseCallbackEvent([]byte(`{"id":"version","event":"input","text":"abc"}`))
	if err != nil {
		t.Fatalf("parse callback event: %v", err)
	}
	input, ok := event.(InputEvent)
	if !ok {
		t.Fatalf("expected InputEvent, got %T", event)
	}
	if input.Text != "abc" {
		t.Fatalf("expected text abc, got %q", input.Text)
	}
}

func TestDropdownSerializesItems(t *testing.T) {
	node := NewDropdown("env", []DropdownItem{{ID: "prod", Label: "Production"}})
	payload, err := json.Marshal(node)
	if err != nil {
		t.Fatalf("marshal dropdown: %v", err)
	}
	var decoded map[string]any
	if err := json.Unmarshal(payload, &decoded); err != nil {
		t.Fatalf("unmarshal dropdown: %v", err)
	}
	if decoded["type"] != "dropdown" {
		t.Fatalf("expected dropdown type, got %v", decoded["type"])
	}
}

func TestRuntimeFlushesRenderedMessages(t *testing.T) {
	applet := newDemoApplet()
	var output bytes.Buffer
	runtime := NewRuntime[demoState](applet, bytes.NewBufferString(""), &output)

	if err := runtime.flush(context.Background()); err != nil {
		t.Fatalf("flush render: %v", err)
	}

	lines := bytes.Split(bytes.TrimSpace(output.Bytes()), []byte("\n"))
	if len(lines) != 3 {
		t.Fatalf("expected 3 messages, got %d", len(lines))
	}
}

func TestSetStateUpdatesRenderedStatus(t *testing.T) {
	applet := newDemoApplet()
	if err := applet.OnCallback(context.Background(), InputEvent{ID: "version", Text: "v2"}); err != nil {
		t.Fatalf("callback: %v", err)
	}
	rendered, err := applet.Render(context.Background())
	if err != nil {
		t.Fatalf("render: %v", err)
	}
	if rendered.Status[0].Text != "v2" {
		t.Fatalf("expected updated status text, got %q", rendered.Status[0].Text)
	}
}

func ptr[T any](value T) *T {
	return &value
}
