from __future__ import annotations

from dataclasses import dataclass

from glimpse_applet import (
    Applet,
    AppletState,
    Box,
    Button,
    Hero,
    Icon,
    Label,
    RenderResult,
    StatusItem,
    click,
)


@dataclass
class CounterState(AppletState):
    count: int = 0


class CounterApplet(Applet[CounterState]):
    def initial_state(self) -> CounterState:
        return CounterState()

    async def render(self) -> RenderResult:
        return RenderResult(
            status=[
                StatusItem(
                    id="counter",
                    icon=Icon.name("view-refresh-symbolic"),
                    text=str(self.state.count),
                )
            ],
            hero=Hero(
                icon=Icon.name("view-refresh-symbolic"),
                title="Counter",
                subtitle=f"Value: {self.state.count}",
            ),
            tree=Box.vertical(
                [
                    Label(f"Count = {self.state.count}"),
                    Button(id="increment", label="Increment"),
                ],
                spacing=8,
            ),
        )

    @click("increment")
    async def on_increment(self, _event) -> None:
        await self.set_state(count=self.state.count + 1)


if __name__ == "__main__":
    CounterApplet().run()
