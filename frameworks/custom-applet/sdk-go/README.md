# Glimpse Applet Go SDK

Small async-style framework for building Glimpse `exec` applets without touching stdio or raw JSON.

## Goals

- typed protocol models
- typed widget builders
- generic stateful applet API
- state-driven rendering via `SetState(...)`
- single `Render()` method returning all panel state

## Example

```go
type CounterState struct {
    Count int
}

type CounterApplet struct {
    sdk.BaseApplet[CounterState]
}

func (a *CounterApplet) Render(context.Context) (sdk.RenderResult, error) {
    return sdk.RenderResult{
        Status: []sdk.StatusItem{
            {ID: "counter", Icon: sdk.IconName("view-refresh-symbolic"), Text: fmt.Sprintf("%d", a.State().Count)},
        },
        Hero: &sdk.Hero{
            Title: "Counter",
            Subtitle: fmt.Sprintf("Value: %d", a.State().Count),
        },
        Tree: ptr(sdk.BoxVertical([]sdk.TreeNode{
            sdk.NewLabel(fmt.Sprintf("Count = %d", a.State().Count)),
            sdk.NewButton("increment", "Increment"),
        }, 8)),
    }, nil
}
```
