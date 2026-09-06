from pathlib import Path

from normen.parser import parse_law_xml

FIXTURES = Path(__file__).parent / "fixtures"


def test_parse_law_metadata_and_numbered_norms() -> None:
    law = parse_law_xml((FIXTURES / "sample.xml").read_bytes())

    assert law.abbreviation == "BGB"
    assert law.title == "Bürgerliches Gesetzbuch"
    assert [norm.citation for norm in law.norms] == [
        "Buch 1",
        "§ 1",
        "(XXXX) §§ 3 bis 6",
        "§ 31a",
        "§ 433",
    ]
    assert law.norms[0].title == "Allgemeiner Teil"
    assert law.norms[0].keys == ()
    assert law.norms[0].text == ""


def test_parse_norm_title_and_paragraph_text() -> None:
    law = parse_law_xml((FIXTURES / "sample.xml").read_bytes())
    first = next(norm for norm in law.norms if norm.citation == "§ 1")

    assert first.title == "Beginn der Rechtsfähigkeit"
    assert first.text == (
        "Die Rechtsfähigkeit des Menschen beginnt mit der Vollendung der Geburt."
    )


def test_parse_multi_paragraph_norm() -> None:
    law = parse_law_xml((FIXTURES / "sample.xml").read_bytes())
    kauf = next(norm for norm in law.norms if norm.citation == "§ 433")

    assert "Kaufvertrag" in kauf.title
    assert kauf.text.startswith("(1) Durch den Kaufvertrag")
    assert "(2) Der Käufer ist verpflichtet" in kauf.text


def test_parse_gg_articles_without_title() -> None:
    law = parse_law_xml((FIXTURES / "gg_sample.xml").read_bytes())

    assert law.abbreviation == "GG"
    assert law.norms[0].citation == "I."
    assert law.norms[0].title == "Die Grundrechte"
    art1 = next(norm for norm in law.norms if norm.citation == "Art 1")
    assert art1.title == ""
    assert "Würde des Menschen ist unantastbar" in art1.text


def test_numbered_list_items_keep_markers() -> None:
    law = parse_law_xml((FIXTURES / "gg_sample.xml").read_bytes())
    art74 = next(norm for norm in law.norms if norm.citation == "Art 74")

    assert "folgende Gebiete:" in art74.text
    assert "1. das bürgerliche Recht;" in art74.text
    assert "2. das Personenstandswesen;" in art74.text
    assert "19a. die wirtschaftliche Sicherung der Krankenhäuser;" in art74.text
    assert "(2) Durch Bundesgesetz" in art74.text
