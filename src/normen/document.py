from __future__ import annotations

import re
from dataclasses import dataclass

from normen.models import CitationKey, Law, Norm

_LIST_ITEM_RE = re.compile(
    r"^(\d+[a-zäöü]?\.|[a-zäöü]\))\s+(.*)$",
    re.IGNORECASE,
)

_CITATION_LABEL = re.compile(
    r"(?:§§?|art(?:ikel)?)\s*(\d+)\s*([a-zäöü])?",
    re.IGNORECASE,
)
_ART_RE = re.compile(r"\bArt(?:ikel)?\.?\s+", re.IGNORECASE)


def format_citation(citation: str, abbreviation: str = "") -> str:
    text = _ART_RE.sub("Art. ", citation)
    if abbreviation and abbreviation.casefold() not in text.casefold():
        if "§" in text or text.startswith("Art."):
            text = f"{text} {abbreviation}"
    return text


def norm_heading(norm: Norm, abbreviation: str = "") -> str:
    heading = format_citation(norm.citation, abbreviation)
    if norm.title:
        return f"{heading}  {norm.title}"
    return heading


def format_norm(norm: Norm, abbreviation: str = "") -> str:
    body = norm.text or "(kein Text)"
    return f"{norm_heading(norm, abbreviation)}\n\n{body}"


def format_law(law: Law) -> str:
    return "\n\n\n".join(format_norm(norm, law.abbreviation) for norm in law.norms)


def _key_label(key: CitationKey) -> str:
    return f"{key.number}{key.suffix}"


def norm_number_label(norm: Norm) -> str:
    if len(norm.keys) > 1:
        return f"{_key_label(norm.keys[0])}-{_key_label(norm.keys[-1])}"
    if norm.keys:
        return _key_label(norm.keys[0])
    match = _CITATION_LABEL.search(norm.citation)
    if match:
        return f"{match.group(1)}{(match.group(2) or '').casefold()}"
    return norm.citation


def law_last_number_label(law: Law) -> str:
    for norm in reversed(law.norms):
        if norm.keys:
            return _key_label(norm.keys[-1])
    return str(len(law.norms))


def _numbered_label(norm: Norm) -> str | None:
    if not norm.keys:
        return None
    return norm_number_label(norm)


def law_pos_current_label(law: Law, current: Norm) -> str:
    numbered = _numbered_label(current)
    if numbered is not None:
        return numbered
    index = next(
        (i for i, norm in enumerate(law.norms) if norm is current or norm.citation == current.citation),
        0,
    )
    for norm in law.norms[index + 1 :]:
        label = _numbered_label(norm)
        if label is not None:
            return label
    for norm in reversed(law.norms[:index]):
        label = _numbered_label(norm)
        if label is not None:
            return label
    return norm_number_label(current)


def law_pos_label(law: Law, current: Norm) -> str:
    return f"{law_pos_current_label(law, current)}:{law_last_number_label(law)}"


@dataclass(frozen=True)
class BodyBlock:
    kind: str
    text: str
    marker: str = ""


def iter_body_blocks(text: str) -> list[BodyBlock]:
    if not text.strip():
        return [BodyBlock("prose", "(kein Text)")]
    blocks: list[BodyBlock] = []
    for paragraph in text.split("\n\n"):
        prose: list[str] = []
        for line in paragraph.split("\n"):
            match = _LIST_ITEM_RE.match(line)
            if match:
                if prose:
                    blocks.append(BodyBlock("prose", "\n".join(prose)))
                    prose = []
                blocks.append(BodyBlock("list", match.group(2), match.group(1)))
            else:
                prose.append(line)
        if prose:
            blocks.append(BodyBlock("prose", "\n".join(prose)))
    return blocks or [BodyBlock("prose", "(kein Text)")]
