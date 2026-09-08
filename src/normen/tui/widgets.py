from __future__ import annotations

from rich.text import Text
from textual import events
from textual.app import ComposeResult
from textual.containers import Horizontal, Vertical
from textual.widgets import Input, Static
from textual.widgets.option_list import Option

from normen.catalog import LawRef
from normen.document import iter_body_blocks, norm_heading
from normen.models import Norm
from normen.search import highlight_text


class CommandInput(Input):
    def __init__(self, **kwargs) -> None:
        kwargs.setdefault("select_on_focus", False)
        super().__init__(**kwargs)

    def check_consume_key(self, key: str, character: str | None) -> bool:
        screen = self.screen
        mode = getattr(screen, "mode", None)
        if mode in {"normal", "para", None}:
            return False
        if mode == "search" and getattr(screen, "search_nav", False):
            return False
        if "+" in key and not key.startswith("shift+"):
            return False
        return character is not None and character.isprintable()

    async def _on_key(self, event: events.Key) -> None:
        mode = getattr(self.screen, "mode", None)
        if mode not in {"normal", "para", None}:
            await super()._on_key(event)
            return
        if not (event.is_printable or event.key == "backspace"):
            return
        event.stop()
        event.prevent_default()
        handler = getattr(self.screen, "on_para_key", None)
        if mode == "para" and handler is not None:
            handler(event)


class NumberedItem(Horizontal):
    def __init__(self, marker: str, text: str) -> None:
        super().__init__(classes="nr-item")
        self.marker = marker
        self.body = text

    def compose(self) -> ComposeResult:
        yield Static(self.marker, classes="nr-mark", markup=False)
        yield Static(self.body, classes="nr-body", markup=False)


class NormBlock(Vertical):
    def __init__(self, norm: Norm, index: int, abbreviation: str = "") -> None:
        super().__init__(
            id=f"n{index}",
            classes="section" if not norm.keys and not norm.text.strip() else "",
        )
        self.norm = norm
        self.norm_index = index
        self.abbreviation = abbreviation

    def compose(self) -> ComposeResult:
        yield Static(
            norm_heading(self.norm, self.abbreviation),
            classes="heading",
            markup=False,
        )
        if not self.norm.text.strip():
            return
        after_list = False
        after_absatz = False
        for block in iter_body_blocks(self.norm.text):
            if block.kind == "list":
                yield NumberedItem(block.marker, block.text)
                after_list = True
                continue
            classes = "body"
            if after_list or after_absatz:
                classes += " spaced"
            yield Static(block.text, classes=classes, markup=False)
            after_list = False
            after_absatz = True


def _law_title(abbreviation: str, title: str) -> str:
    return f"{abbreviation}  {title}"


def _law_option(ref: LawRef, query: str = "") -> Option:
    prompt = Text()
    prompt.append_text(highlight_text(f"{ref.shortcut:<10}", query))
    prompt.append_text(highlight_text(ref.title, query))
    return Option(prompt, id=ref.slug)


def _status_bar() -> ComposeResult:
    with Horizontal(id="status"):
        yield Static("NORMAL", id="mode")
        yield Static("", id="pos")


class TabBar(Static):
    def __init__(self) -> None:
        super().__init__("", id="tabs")

    def on_mount(self) -> None:
        app = self.app
        tab_line = getattr(app, "tab_line", None)
        if callable(tab_line):
            self.update(tab_line())
            self.display = True
