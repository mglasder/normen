from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class LawRef:
    shortcut: str
    slug: str
    title: str
    aliases: tuple[str, ...] = ()


LAWS: tuple[LawRef, ...] = (
    LawRef("BGB", "bgb", "Bürgerliches Gesetzbuch"),
    LawRef("GG", "gg", "Grundgesetz für die Bundesrepublik Deutschland"),
    LawRef("VwGO", "vwgo", "Verwaltungsgerichtsordnung"),
    LawRef("VwVfG", "vwvfg", "Verwaltungsverfahrensgesetz", aliases=("vwvwfg",)),
)


def resolve_law(query: str) -> LawRef | None:
    key = query.strip().casefold()
    if not key:
        return None
    for law in LAWS:
        names = {law.shortcut.casefold(), law.slug.casefold(), *law.aliases}
        if key in names:
            return law
    return None


def filter_laws(query: str, laws: tuple[LawRef, ...] = LAWS) -> list[LawRef]:
    key = query.strip().casefold()
    if not key:
        return list(laws)
    matches = []
    for law in laws:
        haystack = (law.shortcut, law.slug, law.title, *law.aliases)
        if any(key in part.casefold() for part in haystack):
            matches.append(law)
    return matches
