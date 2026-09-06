from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class CitationKey:
    number: int
    suffix: str = ""


@dataclass(frozen=True)
class Norm:
    citation: str
    title: str
    text: str
    keys: tuple[CitationKey, ...]


@dataclass(frozen=True)
class Law:
    abbreviation: str
    title: str
    norms: tuple[Norm, ...]


@dataclass(frozen=True)
class SearchHit:
    norm: Norm
    in_title: bool
    preview: str = ""
    score: float = 0.0
