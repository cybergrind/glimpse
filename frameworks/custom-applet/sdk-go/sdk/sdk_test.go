package sdk

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"strings"
	"testing"
	"time"
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

func (a *demoApplet) OnStart(context.Context) error           { return nil }
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
		Tree: ptr(BoxVertical([]TreeNode{
			NewHero("Demo", a.State().Version),
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
	if len(lines) != 2 {
		t.Fatalf("expected 2 messages, got %d", len(lines))
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

type asyncDemoApplet struct {
	BaseApplet[demoState]
}

func newAsyncDemoApplet() *asyncDemoApplet {
	return &asyncDemoApplet{
		BaseApplet: NewBaseApplet(demoState{Version: "v1"}),
	}
}

func (a *asyncDemoApplet) OnStart(context.Context) error {
	go func() {
		time.Sleep(20 * time.Millisecond)
		a.SetState(func(state *demoState) {
			state.Version = "v2"
		})
	}()
	return nil
}

func (a *asyncDemoApplet) OnInit(context.Context, InitEvent) error         { return nil }
func (a *asyncDemoApplet) OnCallback(context.Context, CallbackEvent) error { return nil }

func (a *asyncDemoApplet) Render(context.Context) (RenderResult, error) {
	return RenderResult{
		Status: []StatusItem{
			{ID: "demo", Icon: IconName("demo-symbolic"), Text: a.State().Version},
		},
	}, nil
}

func TestRuntimeFlushesWhenStateChangesWithoutInput(t *testing.T) {
	inputReader, inputWriter := io.Pipe()
	defer inputWriter.Close()
	outputReader, outputWriter := io.Pipe()
	defer outputReader.Close()

	runtime := NewRuntime[demoState](newAsyncDemoApplet(), inputReader, outputWriter)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	done := make(chan error, 1)
	go func() {
		done <- runtime.Run(ctx)
	}()

	scanner := bufio.NewScanner(outputReader)
	var sawV1 bool
	var sawV2 bool
	deadline := time.After(500 * time.Millisecond)

	for !sawV2 {
		select {
		case <-deadline:
			t.Fatalf("expected async state update to flush output; sawV1=%v sawV2=%v", sawV1, sawV2)
		default:
		}

		if !scanner.Scan() {
			time.Sleep(10 * time.Millisecond)
			continue
		}
		line := scanner.Text()
		if !strings.Contains(line, "\"type\":\"status\"") {
			continue
		}
		if strings.Contains(line, "\"text\":\"v1\"") {
			sawV1 = true
		}
		if strings.Contains(line, "\"text\":\"v2\"") {
			sawV2 = true
		}
	}

	cancel()
	inputWriter.Close()
	outputWriter.Close()

	err := <-done
	if err != nil && !errors.Is(err, context.Canceled) {
		t.Fatalf("runtime returned unexpected error: %v", err)
	}
	if !sawV1 {
		t.Fatal("expected initial render before async update")
	}
}
