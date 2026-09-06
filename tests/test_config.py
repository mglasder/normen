from pathlib import Path

from normen.config import DEFAULT_KEYS, DEFAULT_TEXT, Config


def test_missing_file_uses_defaults(tmp_path: Path) -> None:
    config = Config(tmp_path / "normen.conf")
    keys = config.keymap()
    assert keys["paragraph_prev"] == "K,shift+k"
    assert keys["paragraph_next"] == "J,shift+j"
    assert keys["move_left"] == DEFAULT_KEYS["move_left"]


def test_file_overrides_reader_keys(tmp_path: Path) -> None:
    path = tmp_path / "normen.conf"
    path.write_text("[reader]\nparagraph_prev = x\nparagraph_next = y\n", encoding="utf-8")
    keys = Config(path).keymap()
    assert keys["paragraph_prev"] == "x"
    assert keys["paragraph_next"] == "y"
    assert keys["enter_para"] == "i"


def test_shared_keys_section_overrides_defaults(tmp_path: Path) -> None:
    path = tmp_path / "normen.conf"
    path.write_text("[keys]\nmove_down = d\n", encoding="utf-8")
    assert Config(path).keymap()["move_down"] == "d"


def test_get_reads_future_sections(tmp_path: Path) -> None:
    path = tmp_path / "normen.conf"
    path.write_text("[display]\nruler = on\n", encoding="utf-8")
    config = Config(path)
    assert config.get("display", "ruler") == "on"
    assert config.get("display", "missing", "off") == "off"


def test_paragraph_keys_are_removed_from_line_scroll(tmp_path: Path) -> None:
    path = tmp_path / "normen.conf"
    path.write_text(
        "[keys]\nmove_down = j,J,down\nmove_up = k,K,up\n"
        "[reader]\nparagraph_prev = K\nparagraph_next = J\n",
        encoding="utf-8",
    )
    keys = Config(path).keymap()
    assert "J" not in keys["move_down"].split(",")
    assert "K" not in keys["move_up"].split(",")
    assert keys["paragraph_next"] == "J"
    assert keys["paragraph_prev"] == "K"


def test_ensure_file_writes_defaults_once(tmp_path: Path) -> None:
    path = tmp_path / "nested" / "normen.conf"
    config = Config(path)
    config.ensure_file()
    assert path.read_text(encoding="utf-8") == DEFAULT_TEXT
    path.write_text("[reader]\nback = Q\n", encoding="utf-8")
    config.ensure_file()
    assert "[reader]\nback = Q\n" in path.read_text(encoding="utf-8")
