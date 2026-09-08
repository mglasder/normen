from __future__ import annotations

from textual import events

INSERT_ONLY = {"enter_normal"}
TYPING_ONLY = {"enter_normal"}
SEARCH_EDIT_ACTIONS = {"enter_normal"}
SEARCH_NAV_ACTIONS = {
    "enter_normal",
    "move_down",
    "move_up",
    "confirm",
    "enter_search",
}
MODE_LABELS = {"normal": "NORMAL", "search": "SEARCH", "para": "PARA"}
WINDOW_RADIUS = 16


def window_range(index: int, total: int, radius: int = WINDOW_RADIUS) -> tuple[int, int]:
    if total <= 0:
        return (0, 0)
    index = max(0, min(index, total - 1))
    return (max(0, index - radius), min(total, index + radius + 1))


def _para_char(char: str) -> bool:
    return char.isalnum() or char in {"§", " ", "."}


def _event_matches(event: events.Key, binding: str) -> bool:
    wanted = {part.strip() for part in binding.split(",") if part.strip()}
    if event.key in wanted:
        return True
    return event.character is not None and event.character in wanted


def _is_yes_key(event: events.Key) -> bool:
    if event.key in {"y", "Y"}:
        return True
    return event.character is not None and event.character.lower() == "y"
