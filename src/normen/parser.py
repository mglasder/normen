from __future__ import annotations

import re
from xml.etree import ElementTree as ET

from normen.models import CitationKey, Law, Norm

_SKIP_ENBEZ = {"inhaltsübersicht"}
_RANGE_RE = re.compile(r"§§\s*(\d+)\s*bis\s*(\d+)", re.IGNORECASE)
_SINGLE_RE = re.compile(r"(?:§§?|art(?:ikel)?)\s*(\d+)\s*([a-zäöü])?", re.IGNORECASE)


def parse_law_xml(data: bytes) -> Law:
    root = ET.fromstring(data)
    abbreviation = ""
    title = ""
    norms: list[Norm] = []

    for elem in root.findall("norm"):
        meta = elem.find("metadaten")
        if meta is None:
            continue
        if not abbreviation:
            abbreviation = (meta.findtext("jurabk") or "").strip()
        if not title:
            title = (meta.findtext("langue") or "").strip()

        enbez = (meta.findtext("enbez") or "").strip()
        if enbez.casefold() in _SKIP_ENBEZ:
            continue

        if enbez:
            citation = enbez
            titel = _collapsed(meta.findtext("titel") or "")
            keys = _citation_keys(enbez)
        else:
            section = _section_heading(meta)
            if section is None:
                continue
            citation, titel = section
            keys = ()

        text = _norm_text(elem.find("textdaten"))
        norms.append(
            Norm(
                citation=citation,
                title=titel,
                text=text,
                keys=keys,
            )
        )

    return Law(abbreviation=abbreviation, title=title, norms=tuple(norms))


def _collapsed(text: str) -> str:
    return " ".join(text.split())


def _section_heading(meta: ET.Element) -> tuple[str, str] | None:
    unit = meta.find("gliederungseinheit")
    if unit is None:
        return None
    bez = _collapsed(unit.findtext("gliederungsbez") or "")
    titel = _collapsed(unit.findtext("gliederungstitel") or "")
    if not bez and not titel:
        return None
    if not bez:
        return titel, ""
    return bez, titel


def _citation_keys(enbez: str) -> tuple[CitationKey, ...]:
    range_match = _RANGE_RE.search(enbez)
    if range_match:
        start = int(range_match.group(1))
        end = int(range_match.group(2))
        return tuple(CitationKey(number) for number in range(start, end + 1))

    single = _SINGLE_RE.search(enbez)
    if single:
        suffix = (single.group(2) or "").casefold()
        return (CitationKey(int(single.group(1)), suffix),)
    return ()


def _norm_text(textdaten: ET.Element | None) -> str:
    if textdaten is None:
        return ""
    text_elem = textdaten.find("text")
    if text_elem is None:
        return ""
    content = text_elem.find("Content")
    if content is None:
        return _inner_text(text_elem).strip()

    paragraphs = []
    for paragraph in content.findall("P"):
        piece = _p_text(paragraph)
        if piece:
            paragraphs.append(piece)
    return "\n\n".join(paragraphs)


def _p_text(paragraph: ET.Element) -> str:
    parts: list[str] = []
    if paragraph.text and paragraph.text.strip():
        parts.append(paragraph.text.strip())
    for child in paragraph:
        if child.tag == "DL":
            listing = _dl_text(child)
            if listing:
                parts.append(listing)
        elif child.tag == "BR":
            parts.append("\n")
        else:
            piece = _inner_text(child).strip()
            if piece:
                parts.append(piece)
        if child.tail and child.tail.strip():
            parts.append(child.tail.strip())
    return "\n".join(part for part in parts if part)


def _dl_text(dl: ET.Element) -> str:
    lines: list[str] = []
    marker = ""
    for child in dl:
        if child.tag == "DT":
            marker = _inner_text(child).strip()
            continue
        if child.tag != "DD":
            continue
        nested = child.find("DL")
        body = _p_text(child) if nested is not None else _inner_text(child).strip()
        if marker and body:
            lines.append(f"{marker} {body}")
        elif marker:
            lines.append(marker)
        elif body:
            lines.append(body)
        marker = ""
    return "\n".join(lines)


def _inner_text(elem: ET.Element) -> str:
    parts: list[str] = []

    def walk(node: ET.Element) -> None:
        if node.tag == "BR":
            parts.append("\n")
        if node.text:
            parts.append(node.text)
        for child in node:
            walk(child)
            if child.tail:
                parts.append(child.tail)

    walk(elem)
    return "".join(parts)
