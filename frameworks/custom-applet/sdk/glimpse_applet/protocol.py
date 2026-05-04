from __future__ import annotations

from dataclasses import dataclass


@dataclass(slots=True)
class Icon:
    kind: str
    value: str

    @classmethod
    def name(cls, value: str) -> "Icon":
        return cls(kind="name", value=value)

    @classmethod
    def path(cls, value: str) -> "Icon":
        return cls(kind="path", value=value)

    def to_protocol(self) -> dict[str, str]:
        return {self.kind: self.value}


@dataclass(slots=True)
class StatusItem:
    id: str | None = None
    icon: Icon | None = None
    label: str | None = None
    tooltip: str | None = None

    def to_protocol(self) -> dict[str, object]:
        payload: dict[str, object] = {}
        if self.id is not None:
            payload["id"] = self.id
        if self.icon is not None:
            payload["icon"] = self.icon.to_protocol()
        if self.label is not None:
            payload["label"] = self.label
        if self.tooltip is not None:
            payload["tooltip"] = self.tooltip
        return payload

