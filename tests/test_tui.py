from __future__ import annotations

import asyncio
import io
from pathlib import Path

from rich.console import Console
from textual.widgets import Input, OptionList

from normen.catalog import LawRef, resolve_law
from normen.config import Config
from normen.fetch import LawLibrary
from normen.tui import (
    WINDOW_RADIUS,
    NormenApp,
    NormBlock,
    NumberedItem,
    PickerScreen,
    ReaderScreen,
    window_range,
)

FIXTURES = Path(__file__).parent / "fixtures"


def _library(cache_dir: Path) -> LawLibrary:
    xml = (FIXTURES / "sample.xml").read_bytes()
    return LawLibrary(cache_dir=cache_dir, downloader=lambda slug: xml)


def _config(cache_dir: Path, text: str | None = None) -> Config:
    path = cache_dir / "normen.conf"
    if text is not None:
        path.write_text(text, encoding="utf-8")
    return Config(path)


def _app(cache_dir: Path, **kwargs) -> NormenApp:
    kwargs.setdefault("library", _library(cache_dir))
    kwargs.setdefault("config", _config(cache_dir))
    return NormenApp(**kwargs)


def test_window_range_keeps_a_bounded_slice() -> None:
    assert window_range(0, 80) == (0, WINDOW_RADIUS + 1)
    start, end = window_range(70, 80)
    assert end == 80
    assert end - start <= 2 * WINDOW_RADIUS + 1
    assert start <= 70 < end


def _big_library(cache_dir: Path, count: int = 80) -> LawLibrary:
    body = "\n".join(
        (
            "<norm><metadaten><jurabk>BGB</jurabk>"
            f"<enbez>§ {index}</enbez><titel>T{index}</titel></metadaten>"
            "<textdaten><text format=\"XML\"><Content>"
            f"<P>Text {index}</P></Content></text></textdaten></norm>"
        )
        for index in range(1, count + 1)
    )
    xml = (
        '<?xml version="1.0"?>'
        "<dokumente><norm><metadaten><jurabk>BGB</jurabk>"
        "<langue>Big</langue></metadaten><textdaten/></norm>"
        f"{body}</dokumente>"
    ).encode()
    return LawLibrary(cache_dir=cache_dir, downloader=lambda slug: xml)


def test_picker_hjkl_move_highlight(tmp_path: Path) -> None:
    asyncio.run(_move_highlight(tmp_path))


async def _move_highlight(cache_dir: Path) -> None:
    app = _app(cache_dir)
    async with app.run_test() as pilot:
        assert isinstance(app.screen, PickerScreen)
        assert app.screen.mode == "normal"
        laws = app.screen.query_one("#laws", OptionList)
        assert laws.highlighted == 0
        await pilot.press("j")
        assert laws.highlighted == 1
        await pilot.press("h")
        assert laws.highlighted == 0
        await pilot.press("l")
        assert laws.highlighted == 1
        await pilot.press("k")
        assert laws.highlighted == 0
        assert app.screen.query_one("#cmd", Input).value == ""


def test_picker_insert_mode_types_shortcut(tmp_path: Path) -> None:
    asyncio.run(_type_shortcut(tmp_path))


async def _type_shortcut(cache_dir: Path) -> None:
    app = _app(cache_dir)
    async with app.run_test() as pilot:
        await pilot.press("v")
        assert app.screen.query_one("#cmd", Input).value == ""
        await pilot.press("i")
        assert app.screen.mode == "insert"
        await pilot.press("v", "w", "g", "o")
        assert app.screen.query_one("#cmd", Input).value == "vwgo"
        await pilot.press("escape")
        assert app.screen.mode == "normal"


def test_reader_opens_full_law_in_normal_mode(tmp_path: Path) -> None:
    asyncio.run(_open_full_law(tmp_path))


async def _open_full_law(cache_dir: Path) -> None:
    app = _app(cache_dir, initial_law="bgb")
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        assert screen.mode == "normal"
        assert "BGB" in screen.title
        assert "Bürgerliches Gesetzbuch" in screen.title
        assert str(screen.query_one("#mode").content) == "NORMAL"
        assert str(screen.query_one("#pos").content) == "1:433"
        blocks = list(screen.query(NormBlock))
        citations = [block.norm.citation for block in blocks]
        assert citations[0] == "Buch 1"
        assert "§ 1" in citations
        assert "§ 433" in citations
        book = next(block for block in blocks if block.norm.citation == "Buch 1")
        assert book.norm.title == "Allgemeiner Teil"
        assert list(book.query(".body")) == []
        assert "Kaufvertrag" in blocks[-1].norm.title


def test_reader_jumps_to_paragraph_by_number(tmp_path: Path) -> None:
    asyncio.run(_jump_to_433(tmp_path))


async def _jump_to_433(cache_dir: Path) -> None:
    app = _app(cache_dir, initial_law="bgb")
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        await pilot.press("4")
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        assert screen.mode == "para"
        assert str(screen.query_one("#mode").content) == "PARA"
        await pilot.press("3", "3")
        assert screen.query_one("#cmd", Input).value == "433"
        await pilot.press("enter")
        assert screen.current is not None
        assert screen.current.citation == "§ 433"
        assert screen.mode == "normal"
        assert str(screen.query_one("#mode").content) == "NORMAL"
        assert str(screen.query_one("#pos").content) == "433:433"
        assert screen.query_one("NormBlock.-current", NormBlock).norm.citation == "§ 433"


def test_reader_mounts_only_a_window_of_norms(tmp_path: Path) -> None:
    asyncio.run(_windowed_reader(tmp_path))


async def _windowed_reader(cache_dir: Path) -> None:
    library = _big_library(cache_dir, count=80)
    app = _app(cache_dir, initial_law="bgb", library=library)
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        mounted = list(screen.query(NormBlock))
        assert len(mounted) <= 2 * WINDOW_RADIUS + 1
        assert len(mounted) < 80
        assert screen.current is not None
        assert screen.current.citation == "§ 1"
        await pilot.press("7", "0", "enter")
        await _wait_for_block(screen, pilot, "§ 70")
        later = list(screen.query(NormBlock))
        assert len(later) <= 2 * WINDOW_RADIUS + 1
        assert all(block.norm.citation != "§ 1" for block in later)
        await pilot.press("J")
        await _wait_for_block(screen, pilot, "§ 71")
        await pilot.press("1", "enter")
        await _wait_for_block(screen, pilot, "§ 1")


def test_reader_hjkl_stay_in_normal_mode(tmp_path: Path) -> None:
    asyncio.run(_reader_nav_keys(tmp_path))


async def _reader_nav_keys(cache_dir: Path) -> None:
    app = _app(cache_dir, initial_law="bgb")
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        first = screen.current.citation if screen.current else None
        await pilot.press("j", "j", "k")
        assert screen.mode == "normal"
        assert screen.query_one("#cmd", Input).value == ""
        assert screen.current is not None
        assert screen.current.citation == first
        await pilot.press("l")
        assert screen.current.citation != first


def test_reader_shift_jk_jump_paragraph(tmp_path: Path) -> None:
    asyncio.run(_jk_jump(tmp_path))


async def _jk_jump(cache_dir: Path) -> None:
    app = _app(cache_dir, initial_law="bgb")
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        first = screen.current.citation if screen.current else None
        await pilot.press("J")
        assert screen.current is not None
        assert screen.current.citation != first
        assert str(screen.query_one("#pos").content) == "1:433"
        await pilot.press("K")
        assert screen.current is not None
        assert screen.current.citation == first
        assert str(screen.query_one("#pos").content) == "1:433"
        heading = screen.query_one("NormBlock.-current .heading")
        assert heading.styles.text_style.bold


def test_stale_config_j_still_jumps_paragraph(tmp_path: Path) -> None:
    asyncio.run(_stale_j_config(tmp_path))


async def _stale_j_config(cache_dir: Path) -> None:
    config = _config(
        cache_dir,
        "[keys]\nmove_down = j,J,down\n"
        "[reader]\nparagraph_prev = K\nparagraph_next = J\n",
    )
    app = _app(cache_dir, initial_law="bgb", config=config)
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        first = screen.current.citation if screen.current else None
        await pilot.press("J")
        assert screen.current is not None
        assert screen.current.citation != first
        await pilot.press("K")
        assert screen.current.citation == first


def test_picker_opens_gg_by_shortcut(tmp_path: Path) -> None:
    asyncio.run(_open_gg(tmp_path))


async def _open_gg(cache_dir: Path) -> None:
    xml = (FIXTURES / "gg_sample.xml").read_bytes()
    library = LawLibrary(cache_dir=cache_dir, downloader=lambda slug: xml)
    app = _app(cache_dir, library=library)
    async with app.run_test() as pilot:
        await pilot.press("i", "g", "g", "enter")
        await _wait_for_law(app, pilot, resolve_law("gg"))
        assert isinstance(app.screen, ReaderScreen)
        assert app.screen.ref.shortcut == "GG"


def test_reader_separates_absätze(tmp_path: Path) -> None:
    asyncio.run(_separate_absätze(tmp_path))


async def _separate_absätze(cache_dir: Path) -> None:
    xml = (FIXTURES / "gg_sample.xml").read_bytes()
    library = LawLibrary(cache_dir=cache_dir, downloader=lambda slug: xml)
    app = _app(cache_dir, initial_law="gg", library=library)
    async with app.run_test(size=(80, 24)) as pilot:
        await _wait_for_law(app, pilot, resolve_law("gg"))
        await pilot.pause()
        lines = _screen_text(app).splitlines()
        second = next(i for i, line in enumerate(lines) if "(2) Das Deutsche Volk" in line)
        gap = lines[second - 1]
        assert "Gewalt" not in gap
        assert "(1)" not in gap
        assert "(2)" not in gap


def test_reader_hangs_numbered_list_items(tmp_path: Path) -> None:
    asyncio.run(_numbered_items(tmp_path))


async def _numbered_items(cache_dir: Path) -> None:
    xml = (FIXTURES / "gg_sample.xml").read_bytes()
    library = LawLibrary(cache_dir=cache_dir, downloader=lambda slug: xml)
    app = _app(cache_dir, initial_law="gg", library=library)
    async with app.run_test(size=(80, 30)) as pilot:
        await _wait_for_law(app, pilot, resolve_law("gg"))
        await pilot.press("7", "4", "enter")
        await pilot.pause()
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        assert screen.current is not None
        assert screen.current.citation == "Art 74"
        items = list(screen.query(NumberedItem))
        assert [item.marker for item in items] == ["1.", "2.", "19a."]
        assert "bürgerliche Recht" in items[0].body
        assert items[0].styles.padding.left == 1
        lines = _screen_text(app).splitlines()
        absatz = next(line for line in lines if "(1) Die konkurrierende" in line)
        first = next(line for line in lines if "1. das bürgerliche" in line)
        assert absatz.index("1") == first.index("1")


def test_picker_opens_law_by_shortcut(tmp_path: Path) -> None:
    asyncio.run(_open_via_shortcut(tmp_path))


async def _open_via_shortcut(cache_dir: Path) -> None:
    app = _app(cache_dir)
    async with app.run_test() as pilot:
        await pilot.press("i", "b", "g", "b", "enter")
        await _wait_for_law(app, pilot)
        assert isinstance(app.screen, ReaderScreen)
        assert app.screen.ref.shortcut == "BGB"


def test_slash_filters_paragraphs_and_enter_jumps(tmp_path: Path) -> None:
    asyncio.run(_filter_and_jump(tmp_path))


async def _filter_and_jump(cache_dir: Path) -> None:
    app = _app(cache_dir, initial_law="bgb")
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        await pilot.press("slash")
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        assert screen.mode == "search"
        assert str(screen.query_one("#mode").content) == "SEARCH"
        await pilot.press("k", "a", "u", "f")
        await pilot.pause()
        results = screen.query_one("#results", OptionList)
        assert results.display is True
        assert results.option_count >= 1
        citations = [hit.norm.citation for hit in screen.hits]
        numbers = [hit.norm.keys[0].number for hit in screen.hits if hit.norm.keys]
        assert numbers == sorted(numbers)
        assert "§ 433" in citations
        assert results.option_count == len(screen.hits)
        options = list(results.options)
        assert all(option._divider for option in options[:-1])
        assert not options[-1]._divider
        card = results.get_component_styles("option-list--option").background.hex.casefold()
        current = results.get_component_styles(
            "option-list--option-highlighted"
        ).background.hex.casefold()
        assert card == "#131a21"
        assert current != card
        results.highlighted = citations.index("§ 433")
        await pilot.press("enter")
        await pilot.pause()
        assert screen.mode == "search"
        assert screen.search_nav is True
        await pilot.press("enter")
        await pilot.pause()
        assert screen.mode == "normal"
        assert str(screen.query_one("#mode").content) == "NORMAL"
        assert screen.current is not None
        assert screen.current.citation == "§ 433"
        assert screen.query_one("#scroll").display is True
        current = screen.query_one("NormBlock.-current", NormBlock)
        assert current.norm.citation == "§ 433"


def test_search_jk_moves_highlight(tmp_path: Path) -> None:
    asyncio.run(_search_jk(tmp_path))


async def _search_jk(cache_dir: Path) -> None:
    app = _app(cache_dir, initial_law="bgb")
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        await pilot.press("slash", "j", "k", "d", "i", "e")
        await pilot.pause()
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        assert screen.search_nav is False
        assert screen.query_one("#cmd", Input).value == "jkdie"
        await pilot.press("escape")
        await pilot.press("slash", "d", "i", "e")
        await pilot.pause()
        assert screen.query_one("#cmd", Input).value == "die"
        await pilot.press("enter")
        await pilot.pause()
        assert screen.mode == "search"
        assert screen.search_nav is True
        results = screen.query_one("#results", OptionList)
        assert results.option_count >= 2
        assert results.highlighted == 0
        await pilot.press("j")
        assert results.highlighted == 1
        await pilot.press("k")
        assert results.highlighted == 0
        assert screen.query_one("#cmd", Input).value == "die"
        await pilot.press("slash")
        assert screen.search_nav is False
        await pilot.press("x")
        assert screen.query_one("#cmd", Input).value == "diex"


def test_search_enter_jumps_highlighted_not_first(tmp_path: Path) -> None:
    asyncio.run(_filter_second_hit(tmp_path))


async def _filter_second_hit(cache_dir: Path) -> None:
    app = _app(cache_dir, initial_law="bgb")
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        await pilot.press("slash")
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        await pilot.press("d", "i", "e", "enter")
        await pilot.pause()
        citations = [hit.norm.citation for hit in screen.hits]
        assert citations[0] != "§ 433"
        assert "§ 433" in citations
        for _ in range(citations.index("§ 433")):
            await pilot.press("j")
        await pilot.press("enter")
        await pilot.pause()
        assert screen.mode == "normal"
        assert screen.current is not None
        assert screen.current.citation == "§ 433"
        assert screen.query_one("NormBlock.-current", NormBlock).norm.citation == "§ 433"


def test_reopen_restores_last_paragraph(tmp_path: Path) -> None:
    asyncio.run(_restore_position(tmp_path))


async def _restore_position(cache_dir: Path) -> None:
    library = _library(cache_dir)
    app = _app(cache_dir, initial_law="bgb", library=library)
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        await pilot.press("4", "3", "3", "enter")
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        assert screen.current is not None
        assert screen.current.citation == "§ 433"
        await pilot.press("q")
        assert isinstance(app.screen, PickerScreen)
        await pilot.press("i", "b", "g", "b", "enter")
        await _wait_for_law(app, pilot)
        restored = app.screen
        assert isinstance(restored, ReaderScreen)
        assert restored.current is not None
        assert restored.current.citation == "§ 433"
        current = restored.query_one("NormBlock.-current", NormBlock)
        assert current.norm.citation == "§ 433"
        assert not (cache_dir / "state.json").exists()


def test_fresh_instance_ignores_disk_state(tmp_path: Path) -> None:
    asyncio.run(_ignore_disk_state(tmp_path))


async def _ignore_disk_state(cache_dir: Path) -> None:
    (cache_dir / "state.json").write_text(
        '{"positions": {"bgb": "§ 433"}}\n',
        encoding="utf-8",
    )
    app = _app(cache_dir, initial_law="bgb")
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        assert screen.current is not None
        assert screen.current.citation != "§ 433"


def test_config_file_remaps_paragraph_keys(tmp_path: Path) -> None:
    asyncio.run(_remap_paragraph_keys(tmp_path))


async def _remap_paragraph_keys(cache_dir: Path) -> None:
    config = _config(
        cache_dir,
        "[reader]\nparagraph_prev = x\nparagraph_next = y\n",
    )
    app = _app(cache_dir, initial_law="bgb", config=config)
    async with app.run_test() as pilot:
        await _wait_for_law(app, pilot)
        screen = app.screen
        assert isinstance(screen, ReaderScreen)
        first = screen.current.citation if screen.current else None
        await pilot.press("y")
        assert screen.current is not None
        assert screen.current.citation != first
        await pilot.press("x")
        assert screen.current.citation == first
        await pilot.press("K")
        assert screen.current.citation == first


def _screen_text(app: NormenApp) -> str:
    console = Console(
        width=app.size.width,
        height=app.size.height,
        file=io.StringIO(),
        force_terminal=True,
        record=True,
        color_system=None,
        legacy_windows=False,
        safe_box=False,
    )
    console.print(
        app.screen._compositor.render_update(
            full=True, screen_stack=app._background_screens
        )
    )
    return console.export_text()


async def _wait_for_block(screen: ReaderScreen, pilot, citation: str) -> None:
    for _ in range(40):
        if screen.current is not None and screen.current.citation == citation:
            if any(block.norm.citation == citation for block in screen.query(NormBlock)):
                return
        await pilot.pause(0.05)
    raise AssertionError(f"block {citation!r} did not mount")


async def _wait_for_law(app: NormenApp, pilot, ref: LawRef | None = None) -> None:
    expected = ref or resolve_law("bgb")
    for _ in range(40):
        screen = app.screen
        if isinstance(screen, ReaderScreen) and screen.law is not None:
            if expected is None or screen.ref.slug == expected.slug:
                if screen.current is not None or screen.mode == "search":
                    return
        await pilot.pause(0.05)
    raise AssertionError("law did not load")
