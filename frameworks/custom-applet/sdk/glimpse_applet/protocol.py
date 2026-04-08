from __future__ import annotations

from dataclasses import dataclass


@dataclass(slots=True)
class Icon:
    type: str
    value: str

    @classmethod
    def name(cls, value: str) -> "Icon":
        return cls(type="name", value=value)

    @classmethod
    def path(cls, value: str) -> "Icon":
        return cls(type="path", value=value)

    def to_protocol(self) -> dict[str, str]:
        return {"type": self.type, "value": self.value}


@dataclass(slots=True)
class StatusItem:
    id: str | None = None
    icon: Icon | None = None
    text: str | None = None

    def to_protocol(self) -> dict[str, object]:
        payload: dict[str, object] = {}
        if self.id is not None:
            payload["id"] = self.id
        if self.icon is not None:
            payload["icon"] = self.icon.to_protocol()
        if self.text is not None:
            payload["text"] = self.text
        return payload


