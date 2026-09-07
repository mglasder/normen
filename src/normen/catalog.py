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
    LawRef("BVerfGG", "bverfgg", "Gesetz über das Bundesverfassungsgericht"),
    LawRef(
        "GOBT",
        "btgo_2025",
        "Geschäftsordnung des Deutschen Bundestages",
        aliases=("gobt", "btgo", "go-bt"),
    ),
    LawRef(
        "GOBR",
        "brgo_2025",
        "Geschäftsordnung des Bundesrates",
        aliases=("gobr", "brgo", "go-br"),
    ),
    LawRef(
        "PartG",
        "partg",
        "Gesetz über die politischen Parteien",
        aliases=("parteig", "parteiengesetz", "parteiegesetz"),
    ),
    LawRef(
        "VereinsG",
        "vereinsg",
        "Gesetz zur Regelung des öffentlichen Vereinsrechts",
        aliases=("vereinsgesetz",),
    ),
    LawRef(
        "VersammlG",
        "versammlg",
        "Gesetz über Versammlungen und Aufzüge",
        aliases=("versammlungsgesetz", "versammunglusgesetzt"),
    ),
    LawRef("BauGB", "bbaug", "Baugesetzbuch", aliases=("bbaug", "bbau")),
    LawRef("BauNVO", "baunvo", "Baunutzungsverordnung"),
    LawRef(
        "VwZG",
        "vwzg_2005",
        "Verwaltungszustellungsgesetz",
        aliases=("vwzg",),
    ),
    LawRef("VwVG", "vwvg", "Verwaltungs-Vollstreckungsgesetz"),
    LawRef("StGB", "stgb", "Strafgesetzbuch"),
    LawRef("ZPO", "zpo", "Zivilprozessordnung"),
    LawRef("StPO", "stpo", "Strafprozeßordnung"),
    LawRef("HGB", "hgb", "Handelsgesetzbuch"),
    LawRef(
        "EGBGB",
        "bgbeg",
        "Einführungsgesetz zum Bürgerlichen Gesetzbuche",
        aliases=("egbgb", "bgbeg"),
    ),
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
