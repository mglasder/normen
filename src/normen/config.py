from __future__ import annotations

import configparser
from pathlib import Path

DEFAULT_CONFIG_PATH = Path.home() / ".config" / "normen" / "normen.conf"

DEFAULT_KEYS = {
    "enter_para": "i",
    "enter_normal": "escape",
    "move_left": "h,left",
    "move_down": "j,down",
    "move_up": "k,up",
    "move_right": "l,right",
    "confirm": "enter",
    "quit": "q",
    "enter_search": "/",
    "paragraph_prev": "K,shift+k",
    "paragraph_next": "J,shift+j",
    "goto_top": "g",
    "goto_bottom": "G",
    "next_hit": "n",
    "prev_hit": "shift+n",
    "page_down": "ctrl+d",
    "page_up": "ctrl+u",
    "back": "q",
}

DEFAULT_TEXT = """\
# normen — configuration
# All settings live here. Unknown keys are ignored so this file can grow.

[keys]
enter_para = i
enter_normal = escape
move_left = h,left
move_down = j,down
move_up = k,up
move_right = l,right
confirm = enter

[picker]
quit = q

[reader]
enter_search = /
paragraph_prev = K,shift+k
paragraph_next = J,shift+j
goto_top = g
goto_bottom = G
next_hit = n
prev_hit = shift+n
page_down = ctrl+d
page_up = ctrl+u
back = q
"""


def default_config_path() -> Path:
    return DEFAULT_CONFIG_PATH


class Config:
    def __init__(self, path: Path | None = None) -> None:
        self.path = path or default_config_path()
        self._parser = configparser.ConfigParser(interpolation=None)
        self.load()

    def load(self) -> None:
        self._parser = configparser.ConfigParser(interpolation=None)
        if not self.path.exists():
            return
        try:
            self._parser.read(self.path, encoding="utf-8")
        except (OSError, configparser.Error):
            return

    def ensure_file(self) -> None:
        if self.path.exists():
            return
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text(DEFAULT_TEXT, encoding="utf-8")

    def get(self, section: str, option: str, fallback: str | None = None) -> str | None:
        if not self._parser.has_option(section, option):
            return fallback
        value = _clean(self._parser.get(section, option))
        return value or fallback

    def section(self, name: str) -> dict[str, str]:
        if not self._parser.has_section(name):
            return {}
        cleaned = {key: _clean(value) for key, value in self._parser.items(name)}
        return {key: value for key, value in cleaned.items() if value}

    def keymap(self) -> dict[str, str]:
        resolved = dict(DEFAULT_KEYS)
        overlay = {}
        overlay.update(self.section("keys"))
        overlay.update(self.section("picker"))
        overlay.update(self.section("reader"))
        if "enter_insert" in overlay and "enter_para" not in overlay:
            overlay["enter_para"] = overlay["enter_insert"]
        resolved.update(overlay)
        resolved["enter_insert"] = resolved["enter_para"]
        para_keys = {
            key.strip()
            for name in ("paragraph_prev", "paragraph_next")
            for key in resolved.get(name, "").split(",")
            if key.strip()
        }
        for name in ("move_down", "move_up"):
            kept = [
                key.strip()
                for key in resolved[name].split(",")
                if key.strip() and key.strip() not in para_keys
            ]
            if kept:
                resolved[name] = ",".join(kept)
        return resolved


def _clean(value: str) -> str:
    return value.split("#", 1)[0].strip()
