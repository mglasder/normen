from __future__ import annotations


class SessionStore:
    def __init__(self) -> None:
        self.positions: dict[str, str] = {}

    def get(self, slug: str) -> str | None:
        return self.positions.get(slug)

    def set(self, slug: str, citation: str) -> None:
        self.positions[slug] = citation
