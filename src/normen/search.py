from __future__ import annotations

import re

from rapidfuzz import fuzz
from rich.text import Text

from normen.document import format_citation
from normen.models import CitationKey, Law, Norm, SearchHit

_QUERY_RE = re.compile(
    r"(?:§§?|art(?:ikel)?)?\s*(\d+)\s*([a-zäöü])?\s*$",
    re.IGNORECASE,
)
_MIN_FUZZY_SCORE = 70
_PREVIEW_WIDTH = 80


def parse_citation_query(query: str) -> CitationKey | None:
    match = _QUERY_RE.fullmatch(query.strip())
    if not match:
        return None
    return CitationKey(int(match.group(1)), (match.group(2) or "").casefold())


def lookup_norm(law: Law, query: str) -> Norm | None:
    key = parse_citation_query(query)
    if key is None:
        citation = query.strip()
        for norm in law.norms:
            if norm.citation == citation:
                return norm
        return None
    for norm in law.norms:
        if key in norm.keys:
            return norm
    return None


def search_norms(law: Law, query: str) -> list[SearchHit]:
    needle = query.strip()
    if not needle:
        return []

    folded = needle.casefold()
    hits: list[SearchHit] = []
    for norm in law.norms:
        title_blob = f"{norm.citation} {norm.title}"
        title_score = fuzz.WRatio(needle, title_blob)
        body_score = fuzz.WRatio(needle, norm.text)
        in_title = folded in title_blob.casefold()
        in_body = folded in norm.text.casefold()
        score = max(title_score, body_score)
        if not (in_title or in_body or score >= _MIN_FUZZY_SCORE):
            continue
        hits.append(
            SearchHit(
                norm=norm,
                in_title=in_title or title_score >= body_score,
                preview=make_preview(norm, needle),
                score=float(score),
            )
        )
    hits.sort(key=lambda hit: _norm_number_key(hit.norm))
    return hits


def _norm_number_key(norm: Norm) -> tuple[int, int, str]:
    if not norm.keys:
        return (1, 0, norm.citation)
    first = norm.keys[0]
    return (0, first.number, first.suffix)


def make_preview(norm: Norm, query: str, width: int = _PREVIEW_WIDTH) -> str:
    text = " ".join((norm.text or "").split())
    if not text:
        return norm.title
    folded = text.casefold()
    needle = query.casefold()
    index = folded.find(needle)
    if index < 0:
        return text[:width]
    start = max(0, index - 20)
    snippet = text[start : start + width]
    if start > 0:
        snippet = f"…{snippet}"
    if start + width < len(text):
        snippet = f"{snippet}…"
    return snippet


def format_search_hit(hit: SearchHit, query: str, abbreviation: str = "") -> Text:
    prompt = Text()
    citation = format_citation(hit.norm.citation, abbreviation)
    prompt.append(citation, style="bold")
    if hit.norm.title:
        prompt.append("  ")
        prompt.append_text(highlight_text(hit.norm.title, query))
    preview = hit.preview.strip()
    if preview and preview != hit.norm.title:
        prompt.append("\n")
        prompt.append_text(highlight_text(preview, query))
    return prompt


def highlight_text(text: str, query: str) -> Text:
    result = Text()
    needle = query.strip().casefold()
    if not needle:
        result.append(text)
        return result
    lower = text.casefold()
    start = 0
    while True:
        index = lower.find(needle, start)
        if index < 0:
            result.append(text[start:])
            break
        result.append(text[start:index])
        result.append(text[index : index + len(needle)], style="bold #10171e on #f5d595")
        start = index + len(needle)
    return result
