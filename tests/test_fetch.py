from pathlib import Path

from normen.catalog import resolve_law
from normen.fetch import LawLibrary, default_cache_dir, xml_zip_url


def test_xml_zip_url() -> None:
    assert xml_zip_url("bgb") == "https://www.gesetze-im-internet.de/bgb/xml.zip"


def test_data_lives_in_home_normen() -> None:
    assert default_cache_dir() == Path.home() / ".normen"


def test_library_caches_downloaded_xml(tmp_path: Path) -> None:
    calls: list[str] = []
    fixture = (Path(__file__).parent / "fixtures" / "sample.xml").read_bytes()

    def downloader(slug: str) -> bytes:
        calls.append(slug)
        return fixture

    library = LawLibrary(cache_dir=tmp_path, downloader=downloader)
    first = library.load(resolve_law("bgb"))
    second = library.load(resolve_law("bgb"))

    assert calls == ["bgb"]
    assert first.abbreviation == "BGB"
    assert second.norms[0].citation == "Buch 1"
    assert any(norm.citation == "§ 1" for norm in second.norms)
    assert (tmp_path / "bgb.xml").exists()
