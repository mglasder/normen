from pathlib import Path

from normen.document import (
    format_citation,
    format_law,
    format_norm,
    iter_body_blocks,
    law_pos_label,
    norm_heading,
    norm_number_label,
)
from normen.parser import parse_law_xml

FIXTURES = Path(__file__).parent / "fixtures"


def test_norm_heading_is_citation_and_title() -> None:
    law = parse_law_xml((FIXTURES / "sample.xml").read_bytes())
    first = next(norm for norm in law.norms if norm.citation == "§ 1")
    assert norm_heading(first, "BGB") == "§ 1 BGB  Beginn der Rechtsfähigkeit"
    book = next(norm for norm in law.norms if norm.citation == "Buch 1")
    assert norm_heading(book, "BGB") == "Buch 1  Allgemeiner Teil"


def test_format_norm_includes_citation_title_and_text() -> None:
    law = parse_law_xml((FIXTURES / "sample.xml").read_bytes())
    first = next(norm for norm in law.norms if norm.citation == "§ 1")
    rendered = format_norm(first, "BGB")
    assert rendered.startswith("§ 1 BGB  Beginn der Rechtsfähigkeit")
    assert "Rechtsfähigkeit des Menschen" in rendered


def test_format_law_contains_every_norm() -> None:
    law = parse_law_xml((FIXTURES / "sample.xml").read_bytes())
    rendered = format_law(law)
    assert "Buch 1  Allgemeiner Teil" in rendered
    assert "§ 1 BGB  Beginn der Rechtsfähigkeit" in rendered
    assert "§ 433 BGB  Vertragstypische Pflichten beim Kaufvertrag" in rendered
    assert "Kaufpreis zu zahlen" in rendered


def test_norm_number_label_keeps_letter_suffix() -> None:
    law = parse_law_xml((FIXTURES / "sample.xml").read_bytes())
    numbered = {norm.citation: norm_number_label(norm) for norm in law.norms}
    assert numbered["§ 1"] == "1"
    assert numbered["§ 31a"] == "31a"
    assert numbered["§ 433"] == "433"
    assert numbered["(XXXX) §§ 3 bis 6"] == "3-6"


def test_status_pos_uses_named_numbers_not_list_index() -> None:
    law = parse_law_xml((FIXTURES / "sample.xml").read_bytes())
    lettered = next(norm for norm in law.norms if norm.citation == "§ 31a")
    assert law.norms.index(lettered) + 1 != 31
    assert law_pos_label(law, lettered) == "31a:433"


def test_status_pos_skips_trailing_anhang() -> None:
    law = parse_law_xml((FIXTURES / "gg_sample.xml").read_bytes())
    art74 = next(norm for norm in law.norms if norm.citation == "Art 74")
    assert law.norms[-1].citation == "Anhang EV"
    assert law_pos_label(law, art74) == "74:74"


def test_status_pos_ignores_section_headers() -> None:
    law = parse_law_xml((FIXTURES / "sample.xml").read_bytes())
    book = next(norm for norm in law.norms if norm.citation == "Buch 1")
    assert law_pos_label(law, book) == "1:433"
    gg = parse_law_xml((FIXTURES / "gg_sample.xml").read_bytes())
    section = next(norm for norm in gg.norms if norm.citation == "I.")
    assert law_pos_label(gg, section) == "1:74"
    assert law_pos_label(gg, gg.norms[-1]) == "74:74"


def test_gg_citation_uses_art_period_and_abbreviation() -> None:
    law = parse_law_xml((FIXTURES / "gg_sample.xml").read_bytes())
    art74 = next(norm for norm in law.norms if norm.citation == "Art 74")
    assert format_citation("Art 74", "GG") == "Art. 74 GG"
    assert norm_heading(art74, "GG") == "Art. 74 GG"


def test_body_blocks_split_numbered_items() -> None:
    law = parse_law_xml((FIXTURES / "gg_sample.xml").read_bytes())
    art74 = next(norm for norm in law.norms if norm.citation == "Art 74")
    blocks = iter_body_blocks(art74.text)
    items = [block for block in blocks if block.kind == "list"]
    assert items[0].marker == "1."
    assert items[0].text.startswith("das bürgerliche Recht")
    assert items[1].marker == "2."
    assert items[2].marker == "19a."
    prose = [block.text for block in blocks if block.kind == "prose"]
    assert any("folgende Gebiete:" in text for text in prose)
    assert any("Bundesgesetz" in text for text in prose)
