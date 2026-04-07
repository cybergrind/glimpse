# Glimpse Applet Python SDK

Small async framework for building Glimpse `exec` applets without touching stdio or raw JSON.

## Goals

- typed protocol models
- typed widget builders
- async runtime
- decorator-based callbacks
- state-driven rendering via `await self.set_state(...)`

## Example

```python
from dataclasses import dataclass, field

from glimpse_applet import (
    Applet,
    AppletState,
    Box,
    Button,
    Hero,
    Icon,
    InputEvent,
    Label,
    RenderResult,
    StatusItem,
    click,
    input,
)


@dataclass
class DeployState(AppletState):
    version: str = "2026.04.07"
    status: str = "Ready"


class DeployApplet(Applet[DeployState]):
    def initial_state(self) -> DeployState:
        return DeployState()

    async def render(self) -> RenderResult:
        return RenderResult(
            status=[
                StatusItem(
                    id="deploy",
                    icon=Icon.name("software-update-available-symbolic"),
                    text=self.state.status,
                )
            ],
            hero=Hero(
                icon=Icon.name("software-update-available-symbolic"),
                title="Deploy",
                subtitle=self.state.version,
            ),
            tree=Box.vertical(
                [
                    Label("Version"),
                    Button(id="deploy_now", label="Deploy now"),
                ]
            ),
        )

    @click("deploy_now")
    async def on_deploy(self, _event) -> None:
        await self.set_state(status="Deploying")


if __name__ == "__main__":
    DeployApplet().run()
```
