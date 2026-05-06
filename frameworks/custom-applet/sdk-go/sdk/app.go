package sdk

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"sync"
)

type RenderResult struct {
	Status []StatusItem
	Tree   *TreeNode
}

type Applet[S any] interface {
	State() *S
	SetState(func(*S))
	OnStart(context.Context) error
	OnInit(context.Context, InitEvent) error
	OnCallback(context.Context, CallbackEvent) error
	Render(context.Context) (RenderResult, error)
}

type BaseApplet[S any] struct {
	mu          sync.RWMutex
	state       S
	popoverOpen bool
	updates     chan struct{}
}

func NewBaseApplet[S any](state S) BaseApplet[S] {
	return BaseApplet[S]{
		state:   state,
		updates: make(chan struct{}, 1),
	}
}

func (a *BaseApplet[S]) State() *S {
	return &a.state
}

func (a *BaseApplet[S]) SetState(patch func(*S)) {
	a.mu.Lock()
	patch(&a.state)
	a.mu.Unlock()
	select {
	case a.updates <- struct{}{}:
	default:
	}
}

func (a *BaseApplet[S]) Snapshot() S {
	a.mu.RLock()
	defer a.mu.RUnlock()
	return a.state
}

func (a *BaseApplet[S]) IsPopoverOpen() bool {
	a.mu.RLock()
	defer a.mu.RUnlock()
	return a.popoverOpen
}

func (a *BaseApplet[S]) SetPopoverOpen(open bool) {
	a.mu.Lock()
	changed := a.popoverOpen != open
	a.popoverOpen = open
	a.mu.Unlock()
	if changed {
		select {
		case a.updates <- struct{}{}:
		default:
		}
	}
}

func (a *BaseApplet[S]) Updates() <-chan struct{} {
	return a.updates
}

type treePayload struct {
	Root *TreeNode `json:"root"`
}

type Runtime[S any] struct {
	applet Applet[S]
	reader io.Reader
	writer io.Writer
	mu     sync.Mutex

	lastStatus  []StatusItem
	lastTree    *treePayload
	popoverOpen bool
}

func NewRuntime[S any](applet Applet[S], reader io.Reader, writer io.Writer) *Runtime[S] {
	return &Runtime[S]{applet: applet, reader: reader, writer: writer}
}

func Run[S any](ctx context.Context, applet Applet[S]) error {
	return NewRuntime(applet, os.Stdin, os.Stdout).Run(ctx)
}

func (r *Runtime[S]) Run(ctx context.Context) error {
	if err := r.applet.OnStart(ctx); err != nil {
		return err
	}
	if err := r.flush(ctx); err != nil {
		return err
	}

	eventCh := make(chan incomingMessage)
	scanErrCh := make(chan error, 1)
	go r.scanInput(ctx, eventCh, scanErrCh)

	var updates <-chan struct{}
	if notifier, ok := r.applet.(interface{ Updates() <-chan struct{} }); ok {
		updates = notifier.Updates()
	}

	for {
		select {
		case <-ctx.Done():
			return ctx.Err()
		case err, ok := <-scanErrCh:
			if ok && err != nil {
				return err
			}
			scanErrCh = nil
			eventCh = nil
			return nil
		case msg, ok := <-eventCh:
			if !ok {
				eventCh = nil
				if scanErrCh == nil {
					return nil
				}
				continue
			}
			switch msg.Type {
			case "init":
				event, err := parseInitEvent(msg.Data)
				if err != nil {
					return err
				}
				if err := r.applet.OnInit(ctx, event); err != nil {
					return err
				}
			case "event":
				event, err := parseCallbackEvent(msg.Data)
				if err != nil {
					return err
				}
				if popoverEvent, ok := event.(PopoverEvent); ok {
					r.setPopoverOpen(popoverEvent.Open)
				}
				if err := r.applet.OnCallback(ctx, event); err != nil {
					return err
				}
			default:
				continue
			}
			if err := r.flush(ctx); err != nil {
				return err
			}
		case <-updates:
			if err := r.flush(ctx); err != nil {
				return err
			}
		}
	}
}

func (r *Runtime[S]) scanInput(
	ctx context.Context,
	eventCh chan<- incomingMessage,
	errCh chan<- error,
) {
	defer close(eventCh)
	defer close(errCh)

	scanner := bufio.NewScanner(r.reader)
	for scanner.Scan() {
		line := append([]byte(nil), scanner.Bytes()...)
		if len(line) == 0 {
			continue
		}
		msg, err := parseIncomingLine(line)
		if err != nil {
			errCh <- err
			return
		}
		if msg.Type == "" {
			continue
		}
		select {
		case <-ctx.Done():
			return
		case eventCh <- msg:
		}
	}

	if err := scanner.Err(); err != nil {
		errCh <- err
	}
}

func (r *Runtime[S]) flush(ctx context.Context) error {
	_ = ctx
	rendered, err := r.applet.Render(context.Background())
	if err != nil {
		return err
	}
	if !statusEqual(r.lastStatus, rendered.Status) {
		if err := r.writeMessage("status", map[string]any{"items": rendered.Status}); err != nil {
			return err
		}
		r.lastStatus = append([]StatusItem(nil), rendered.Status...)
	}
	tree := &treePayload{Root: rendered.Tree}
	if !r.popoverOpen && r.lastTree != nil && tree.Root != nil {
		return nil
	}
	if !treePayloadEqual(r.lastTree, tree) {
		if err := r.writeMessage("popover", tree); err != nil {
			return err
		}
		r.lastTree = tree
	}
	return nil
}

func (r *Runtime[S]) setPopoverOpen(open bool) {
	r.popoverOpen = open
	if stateful, ok := r.applet.(interface{ SetPopoverOpen(bool) }); ok {
		stateful.SetPopoverOpen(open)
	}
}

func (r *Runtime[S]) writeMessage(kind string, data any) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	encoded, err := json.Marshal(data)
	if err != nil {
		return err
	}
	if _, err := fmt.Fprintf(r.writer, "%s %s\n", kind, encoded); err != nil {
		return err
	}
	return nil
}

func statusEqual(left, right []StatusItem) bool {
	encodedLeft, _ := json.Marshal(left)
	encodedRight, _ := json.Marshal(right)
	return string(encodedLeft) == string(encodedRight)
}

func treePayloadEqual(left, right *treePayload) bool {
	encodedLeft, _ := json.Marshal(left)
	encodedRight, _ := json.Marshal(right)
	return string(encodedLeft) == string(encodedRight)
}
