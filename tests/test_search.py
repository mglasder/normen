from pathlib import Path

from normen.models import CitationKey, Law, Norm
from normen.parser import parse_law_xml
from normen.search import format_search_hit, highlight_text, lookup_norm, search_norms

FIXTURES = Path(__file__).parent / "fixtures"


def _bgb():
    return parse_law_xml((FIXTURES / "sample.xml").read_bytes())


def _gg():
    return parse_law_xml((FIXTURES / "gg_sample.xml").read_bytes())


def test_lookup_by_bare_number() -> None:
    assert lookup_norm(_bgb(), "1").citation == "§ 1"
    assert lookup_norm(_bgb(), "433").citation == "§ 433"


def test_lookup_accepts_paragraph_prefix_and_letter_suffix() -> None:
    assert lookup_norm(_bgb(), "§ 31a").citation == "§ 31a"
    assert lookup_norm(_bgb(), "31A").citation == "§ 31a"


def test_lookup_maps_repealed_range() -> None:
    assert lookup_norm(_bgb(), "4").citation == "(XXXX) §§ 3 bis 6"


def test_lookup_gg_article() -> None:
    assert lookup_norm(_gg(), "1").citation == "Art 1"
    assert lookup_norm(_gg(), "art 20").citation == "Art 20"


def test_lookup_unknown_number_returns_none() -> None:
    assert lookup_norm(_bgb(), "9999") is None


def test_full_text_search_finds_title_and_body() -> None:
    hits = search_norms(_bgb(), "Kaufvertrag")
    assert [hit.norm.citation for hit in hits] == ["§ 433"]
    assert hits[0].in_title is True


def test_full_text_search_is_case_insensitive() -> None:
    hits = search_norms(_gg(), "würde")
    assert hits[0].norm.citation == "Art 1"
    assert hits[0].in_title is False


def test_search_results_sorted_by_norm_number() -> None:
    law = Law(
        abbreviation="BGB",
        title="Test",
        norms=(
            Norm("§ 433", "Kauf", "text", (CitationKey(433),)),
            Norm("§ 1", "Beginn", "kauf im text", (CitationKey(1),)),
            Norm("§ 31a", "Haftung", "auch kauf", (CitationKey(31, "a"),)),
        ),
    )
    hits = search_norms(law, "kauf")
    assert [hit.norm.citation for hit in hits] == ["§ 1", "§ 31a", "§ 433"]


def test_fuzzy_search_finds_partial_title() -> None:
    hits = search_norms(_bgb(), "kaufvertr")
    assert hits[0].norm.citation == "§ 433"
    assert "Kaufvertrag" in hits[0].preview or "Kaufvertrag" in hits[0].norm.title


def test_search_hit_includes_preview() -> None:
    hits = search_norms(_bgb(), "Kaufvertrag")
    assert hits[0].preview
    assert "Kaufvertrag" in hits[0].preview or "Kaufvertrag" in hits[0].norm.title


def test_search_hit_prompt_bolds_citation_and_separates_preview() -> None:
    hits = search_norms(_bgb(), "Kaufvertrag")
    prompt = format_search_hit(hits[0], "Kaufvertrag", "BGB")
    lines = prompt.plain.splitlines()
    assert lines[0].startswith("§ 433 BGB")
    assert "Kaufvertrag" in prompt.plain
    citation_end = len("§ 433 BGB")
    assert any(
        span.start == 0
        and span.end == citation_end
        and span.style
        and "bold" in str(span.style)
        for span in prompt.spans
    )
    assert len(lines) >= 2


def test_highlight_text_marks_query() -> None:
    rendered = highlight_text("Vertragstypische Pflichten beim Kaufvertrag", "Kauf")
    plain = rendered.plain
    assert "Kaufvertrag" in plain
    assert any(span.style and "f5d595" in str(span.style) for span in rendered.spans)
    assert any(span.style and "on #" in str(span.style).casefold() for span in rendered.spans)
