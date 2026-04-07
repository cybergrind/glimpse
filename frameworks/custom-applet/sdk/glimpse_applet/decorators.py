from __future__ import annotations

from collections.abc import Callable
from typing import Any


def event(event_type: str, target_id: str) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    def decorator(func: Callable[..., Any]) -> Callable[..., Any]:
        setattr(func, "__glimpse_handler__", (event_type, target_id))
        return func

    return decorator


def click(target_id: str) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    return event("click", target_id)


def input(target_id: str) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    return event("input", target_id)


def change(target_id: str) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    return event("change", target_id)


def toggle(target_id: str) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    return event("toggle", target_id)


def scroll(target_id: str) -> Callable[[Callable[..., Any]], Callable[..., Any]]:
    return event("scroll", target_id)
