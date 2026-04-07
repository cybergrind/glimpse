package main

import (
	"context"
	"fmt"

	sdk "github.com/glimpse-project/custom-applet-sdk-go/sdk"
)

type counterState struct {
	Count int
}

type counterApplet struct {
	sdk.BaseApplet[counterState]
}

func newCounterApplet() *counterApplet {
	return &counterApplet{
		BaseApplet: sdk.NewBaseApplet(counterState{}),
	}
}

func (a *counterApplet) OnStart(context.Context) error { return nil }
func (a *counterApplet) OnInit(context.Context, sdk.InitEvent) error { return nil }

func (a *counterApplet) OnCallback(_ context.Context, event sdk.CallbackEvent) error {
	if click, ok := event.(sdk.ClickEvent); ok && click.ID == "increment" {
		a.SetState(func(state *counterState) {
			state.Count++
		})
	}
	return nil
}

func (a *counterApplet) Render(context.Context) (sdk.RenderResult, error) {
	return sdk.RenderResult{
		Status: []sdk.StatusItem{
			{ID: "counter", Icon: sdk.IconName("view-refresh-symbolic"), Text: fmt.Sprintf("%d", a.State().Count)},
		},
		Hero: &sdk.Hero{
			Title:    "Counter",
			Subtitle: fmt.Sprintf("Value: %d", a.State().Count),
			Icon:     sdk.IconName("view-refresh-symbolic"),
		},
		Tree: ptr(sdk.BoxVertical([]sdk.TreeNode{
			sdk.NewLabel(fmt.Sprintf("Count = %d", a.State().Count)),
			sdk.NewButton("increment", "Increment"),
		}, 8)),
	}, nil
}

func main() {
	if err := sdk.Run[counterState](context.Background(), newCounterApplet()); err != nil {
		panic(err)
	}
}

func ptr[T any](value T) *T {
	return &value
}
