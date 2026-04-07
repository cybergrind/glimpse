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
	Hero   *Hero
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
	state S
}

func NewBaseApplet[S any](state S) BaseApplet[S] {
	return BaseApplet[S]{state: state}
}

func (a *BaseApplet[S]) State() *S {
	return &a.state
}

func (a *BaseApplet[S]) SetState(patch func(*S)) {
	patch(&a.state)
}

type Runtime[S any] struct {
	applet Applet[S]
	reader io.Reader
	writer io.Writer
	mu     sync.Mutex

	lastStatus []StatusItem
	lastHero   *Hero
	lastTree   *TreeNode
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

	scanner := bufio.NewScanner(r.reader)
	for scanner.Scan() {
		line := scanner.Bytes()
		if len(line) == 0 {
			continue
		}
		var msg incomingMessage
		if err := json.Unmarshal(line, &msg); err != nil {
			return err
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
		case "callback":
			event, err := parseCallbackEvent(msg.Data)
			if err != nil {
				return err
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
	}
	return scanner.Err()
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
	if !heroEqual(r.lastHero, rendered.Hero) {
		if err := r.writeMessage("hero", rendered.Hero); err != nil {
			return err
		}
		r.lastHero = rendered.Hero
	}
	if !treeEqual(r.lastTree, rendered.Tree) {
		if err := r.writeMessage("tree", map[string]any{"content": rendered.Tree}); err != nil {
			return err
		}
		r.lastTree = rendered.Tree
	}
	return nil
}

func (r *Runtime[S]) writeMessage(kind string, data any) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	payload := map[string]any{
		"type": kind,
		"data": data,
	}
	encoded, err := json.Marshal(payload)
	if err != nil {
		return err
	}
	if _, err := fmt.Fprintf(r.writer, "%s\n", encoded); err != nil {
		return err
	}
	return nil
}

func statusEqual(left, right []StatusItem) bool {
	encodedLeft, _ := json.Marshal(left)
	encodedRight, _ := json.Marshal(right)
	return string(encodedLeft) == string(encodedRight)
}

func heroEqual(left, right *Hero) bool {
	encodedLeft, _ := json.Marshal(left)
	encodedRight, _ := json.Marshal(right)
	return string(encodedLeft) == string(encodedRight)
}

func treeEqual(left, right *TreeNode) bool {
	encodedLeft, _ := json.Marshal(left)
	encodedRight, _ := json.Marshal(right)
	return string(encodedLeft) == string(encodedRight)
}
