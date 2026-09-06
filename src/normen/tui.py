from __future__ import annotations

from textual import events, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, ScrollableContainer, Vertical
from textual.screen import Screen
from textual.widgets import Header, Input, OptionList, Static
from textual.widgets.option_list import Option

from normen.catalog import LAWS, LawRef, resolve_law
from normen.config import Config
from normen.document import iter_body_blocks, law_pos_label, norm_heading
from normen.fetch import LawLibrary
from normen.models import Law, Norm, SearchHit
from normen.search import format_search_hit, lookup_norm, search_norms
from normen.session import SessionStore
from normen.theme import PASTEL_DARK

INSERT_ONLY = {"enter_normal"}
TYPING_ONLY = {"enter_normal"}
SEARCH_EDIT_ACTIONS = {"enter_normal"}
SEARCH_NAV_ACTIONS = {
    "enter_normal",
    "move_down",
    "move_up",
    "confirm",
    "enter_search",
}
MODE_LABELS = {"normal": "NORMAL", "search": "SEARCH", "para": "PARA"}
WINDOW_RADIUS = 16


def window_range(index: int, total: int, radius: int = WINDOW_RADIUS) -> tuple[int, int]:
    if total <= 0:
        return (0, 0)
    index = max(0, min(index, total - 1))
    return (max(0, index - radius), min(total, index + radius + 1))


class CommandInput(Input):
    def __init__(self, **kwargs) -> None:
        kwargs.setdefault("select_on_focus", False)
        super().__init__(**kwargs)

    def check_consume_key(self, key: str, character: str | None) -> bool:
        screen = self.screen
        mode = getattr(screen, "mode", None)
        if mode == "para":
            return False
        if mode == "search" and getattr(screen, "search_nav", False):
            return False
        return character is not None and character.isprintable()


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


def _para_char(char: str) -> bool:
    return char.isalnum() or char in {"§", " ", "."}


def _status_bar() -> ComposeResult:
    with Horizontal(id="status"):
        yield Static("NORMAL", id="mode")
        yield Static("", id="pos")


class PickerScreen(Screen):
    BINDINGS = [
        Binding("i", "enter_insert", "Insert", id="enter_insert", priority=True),
        Binding("escape", "enter_normal", "Normal", show=False, id="enter_normal", priority=True),
        Binding("h,left", "move_left", "H", id="move_left", priority=True),
        Binding("j,down", "move_down", "J", id="move_down", priority=True),
        Binding("k,up", "move_up", "K", id="move_up", priority=True),
        Binding("l,right", "move_right", "L", id="move_right", priority=True),
        Binding("enter", "confirm", "Öffnen", id="confirm", priority=True),
        Binding("q", "app.quit", "Beenden", id="quit", priority=True),
    ]

    def __init__(self) -> None:
        super().__init__()
        self.mode = "normal"
        self.filtering = False

    def compose(self) -> ComposeResult:
        yield Header(show_clock=False, icon="")
        yield CommandInput(placeholder="", id="cmd")
        with Vertical(id="picker-body"):
            yield OptionList(id="laws")
        yield from _status_bar()

    def on_mount(self) -> None:
        law_list = self.query_one("#laws", OptionList)
        law_list.add_options(
            [Option(f"{ref.shortcut:<8}{ref.title}", id=ref.slug) for ref in LAWS]
        )
        law_list.highlighted = 0
        self.title = "normen"
        self.sub_title = ""
        self._set_mode("normal")

    def check_action(self, action: str, parameters: tuple) -> bool | None:
        if self.mode == "insert" and action not in INSERT_ONLY:
            return False
        return True

    def action_enter_insert(self) -> None:
        self._set_mode("insert")

    def action_enter_normal(self) -> None:
        self.query_one("#cmd", CommandInput).value = ""
        self._set_mode("normal")

    def action_move_down(self) -> None:
        self.query_one("#laws", OptionList).action_cursor_down()

    def action_move_up(self) -> None:
        self.query_one("#laws", OptionList).action_cursor_up()

    def action_move_left(self) -> None:
        self.action_move_up()

    def action_move_right(self) -> None:
        self.action_move_down()

    def action_confirm(self) -> None:
        self._open_highlighted()

    def on_input_submitted(self, event: Input.Submitted) -> None:
        query = event.value.strip()
        event.input.value = ""
        if not query:
            self._set_mode("normal")
            self._open_highlighted()
            return
        ref = resolve_law(query)
        if ref is None:
            event.input.placeholder = f"Unbekanntes Gesetzbuch: {query}"
            self._set_mode("insert")
            return
        self._set_mode("normal")
        self._open(ref)

    def on_option_list_option_selected(self, event: OptionList.OptionSelected) -> None:
        ref = resolve_law(event.option.id or "")
        if ref is not None:
            self._open(ref)

    def _set_mode(self, mode: str) -> None:
        self.mode = mode
        bar = self.query_one("#status")
        label = self.query_one("#mode", Static)
        inp = self.query_one("#cmd", CommandInput)
        if mode == "insert":
            bar.add_class("insert")
            label.update("INSERT")
            inp.focus()
        else:
            bar.remove_class("insert")
            label.update("NORMAL")
            inp.blur()
            self.query_one("#laws", OptionList).focus()

    def _open_highlighted(self) -> None:
        law_list = self.query_one("#laws", OptionList)
        if law_list.highlighted is None:
            return
        option = law_list.get_option_at_index(law_list.highlighted)
        ref = resolve_law(option.id or "")
        if ref is not None:
            self._open(ref)

    def _open(self, ref: LawRef) -> None:
        app = self.app
        assert isinstance(app, NormenApp)
        app.push_screen(
            ReaderScreen(ref, app.library, app.session, app.refresh_remote)
        )


class ReaderScreen(Screen):
    BINDINGS = [
        Binding("i", "enter_para", "Para", id="enter_para", priority=True),
        Binding("slash", "enter_search", "Suche", id="enter_search", priority=True),
        Binding("escape", "enter_normal", "Normal", show=False, id="enter_normal", priority=True),
        Binding("h,left", "move_left", "H", id="move_left", priority=True),
        Binding("j,down", "move_down", "J", id="move_down", priority=True),
        Binding("k,up", "move_up", "K", id="move_up", priority=True),
        Binding("l,right", "move_right", "L", id="move_right", priority=True),
        Binding("J,shift+j", "paragraph_next", "Absatz ↓", show=False, id="paragraph_next", priority=True),
        Binding("K,shift+k", "paragraph_prev", "Absatz ↑", show=False, id="paragraph_prev", priority=True),
        Binding("g", "goto_top", "Anfang", show=False, id="goto_top", priority=True),
        Binding("G", "goto_bottom", "Ende", show=False, id="goto_bottom", priority=True),
        Binding("n", "next_hit", "Nächster", id="next_hit", priority=True),
        Binding("shift+n", "prev_hit", "Vorheriger", id="prev_hit", priority=True),
        Binding("ctrl+d", "page_down", show=False, id="page_down", priority=True),
        Binding("ctrl+u", "page_up", show=False, id="page_up", priority=True),
        Binding("enter", "confirm", "Öffnen", show=False, id="confirm", priority=True),
        Binding("q", "back", "Zurück", id="back", priority=True),
    ] + [
        Binding(digit, f"type_digit('{digit}')", show=False, priority=True)
        for digit in "0123456789"
    ]

    def __init__(
        self,
        ref: LawRef,
        library: LawLibrary,
        session: SessionStore,
        refresh: bool = False,
        initial_query: str | None = None,
    ) -> None:
        super().__init__()
        self.ref = ref
        self.library = library
        self.session = session
        self.refresh_remote = refresh
        self.initial_query = initial_query
        self.mode = "normal"
        self.search_nav = False
        self.law: Law | None = None
        self.current: Norm | None = None
        self.hits: list[SearchHit] = []
        self.hit_index = -1
        self._current_block: NormBlock | None = None
        self._current_index = -1
        self._mounted: tuple[int, int] = (0, 0)
        self._para = ""
        self._pending_pin_top = True
        self._pin_retries = 0

    @property
    def filtering(self) -> bool:
        return self.mode == "search"

    def compose(self) -> ComposeResult:
        yield Header(show_clock=False, icon="")
        yield CommandInput(placeholder="", id="cmd")
        with ScrollableContainer(id="scroll"):
            yield Static(
                f"Lade {self.ref.shortcut} von gesetze-im-internet.de …", id="loading"
            )
        yield OptionList(id="results")
        yield from _status_bar()

    def on_mount(self) -> None:
        self.title = _law_title(self.ref.shortcut, self.ref.title)
        self.sub_title = ""
        self.query_one("#results", OptionList).display = False
        self._set_mode("normal")
        self.load_law()

    def check_action(self, action: str, parameters: tuple) -> bool | None:
        if action == "type_digit":
            return self.mode in {"normal", "para"}
        if self.mode == "search":
            allowed = SEARCH_NAV_ACTIONS if self.search_nav else SEARCH_EDIT_ACTIONS
            return action in allowed
        if self.mode == "para" and action in {"enter_normal", "confirm"}:
            return True
        if self.mode == "para" and action not in TYPING_ONLY:
            return False
        return True

    def on_key(self, event: events.Key) -> None:
        if self.mode != "para":
            return
        if event.key == "backspace":
            event.stop()
            self._para = self._para[:-1]
            self._sync_para()
            return
        char = event.character
        if char and _para_char(char) and not char.isdigit():
            event.stop()
            self._para += char
            self._sync_para()

    def action_type_digit(self, digit: str) -> None:
        if self.mode == "para":
            self._para += digit
        else:
            self._para = digit
            self._set_mode("para")
        self._sync_para()

    @work(thread=True, exclusive=True)
    def load_law(self) -> None:
        try:
            law = self.library.load(self.ref, refresh=self.refresh_remote)
        except Exception as exc:
            self.app.call_from_thread(self._show_error, str(exc))
            return
        self.app.call_from_thread(self._on_loaded, law)

    def _show_error(self, message: str) -> None:
        try:
            self.query_one("#loading", Static).update(f"Fehler beim Laden:\n{message}")
        except Exception:
            pass

    def _on_loaded(self, law: Law) -> None:
        self.law = law
        self.title = _law_title(law.abbreviation, law.title)
        self.sub_title = ""
        scroll = self.query_one("#scroll", ScrollableContainer)
        scroll.remove_children()
        self._current_block = None
        self._current_index = -1
        self._mounted = (0, 0)
        self.call_after_refresh(self._apply_opening_position)

    def _apply_opening_position(self) -> None:
        if self.law is None:
            return
        if self.initial_query:
            query = self.initial_query.strip()
            if query.startswith("/"):
                self.action_enter_search()
                needle = query[1:]
                self.query_one("#cmd", CommandInput).value = needle
                self._render_results(needle)
                return
            self._run_query(query)
            return
        saved = self.session.get(self.ref.slug)
        if saved:
            norm = lookup_norm(self.law, saved)
            if norm is not None:
                self._select_norm(norm)
                return
        if self.law.norms:
            self._select_index(0, pin_top=True)

    def action_enter_para(self) -> None:
        self._enter_para()

    def action_enter_search(self) -> None:
        if self.mode == "search":
            self.search_nav = False
            self._set_mode("search")
            return
        self.search_nav = False
        self.query_one("#results", OptionList).display = True
        self.query_one("#scroll").display = False
        self.query_one("#cmd", CommandInput).value = ""
        self._render_results("")
        self._set_mode("search")

    def action_enter_normal(self) -> None:
        self._para = ""
        self.search_nav = False
        self.query_one("#cmd", CommandInput).value = ""
        if self.mode == "search":
            self._close_filter()
        self._set_mode("normal")

    def _enter_para(self, seed: str = "") -> None:
        self._para = seed
        self._set_mode("para")
        self._sync_para()

    def _sync_para(self) -> None:
        self.query_one("#cmd", CommandInput).value = self._para

    def action_back(self) -> None:
        self._remember_position()
        self.app.pop_screen()

    def action_move_down(self) -> None:
        if self.mode == "search":
            self._step_result(1)
            return
        self.query_one("#scroll", ScrollableContainer).scroll_down()

    def action_move_up(self) -> None:
        if self.mode == "search":
            self._step_result(-1)
            return
        self.query_one("#scroll", ScrollableContainer).scroll_up()

    def _step_result(self, delta: int) -> None:
        results = self.query_one("#results", OptionList)
        if not results.option_count:
            return
        current = 0 if results.highlighted is None else results.highlighted
        results.highlighted = max(0, min(current + delta, results.option_count - 1))

    def action_move_left(self) -> None:
        self._step_norm(-1)

    def action_move_right(self) -> None:
        self._step_norm(1)

    def action_paragraph_prev(self) -> None:
        self._step_norm(-1)

    def action_paragraph_next(self) -> None:
        self._step_norm(1)

    def action_page_down(self) -> None:
        self.query_one("#scroll", ScrollableContainer).scroll_page_down()

    def action_page_up(self) -> None:
        self.query_one("#scroll", ScrollableContainer).scroll_page_up()

    def action_goto_top(self) -> None:
        if self.law and self.law.norms:
            self._select_index(0, pin_top=True)
        self.query_one("#scroll", ScrollableContainer).scroll_home()

    def action_goto_bottom(self) -> None:
        if self.law and self.law.norms:
            self._select_index(len(self.law.norms) - 1, pin_top=True)
        self.query_one("#scroll", ScrollableContainer).scroll_end()

    def action_next_hit(self) -> None:
        self._step_hit(1)

    def action_prev_hit(self) -> None:
        self._step_hit(-1)

    def action_confirm(self) -> None:
        if self.mode == "search" and self.search_nav:
            self._open_highlighted_hit()
        elif self.mode == "para":
            self._confirm_para()

    def on_input_changed(self, event: Input.Changed) -> None:
        if self.mode != "search" or self.search_nav:
            return
        self._render_results(event.value.lstrip("/"))

    def on_input_submitted(self, event: Input.Submitted) -> None:
        if self.mode == "search" and not self.search_nav:
            self._lock_search()
            return
        if self.mode == "para":
            self._confirm_para()
            return
        event.input.value = ""
        self._set_mode("normal")
        query = event.value.strip()
        if query:
            self._run_query(query)

    def _confirm_para(self) -> None:
        query = self._para.strip() or self.query_one("#cmd", CommandInput).value.strip()
        self._para = ""
        self.query_one("#cmd", CommandInput).value = ""
        self._set_mode("normal")
        if query:
            self._run_query(query)

    def on_option_list_option_selected(self, event: OptionList.OptionSelected) -> None:
        if event.option_list.id != "results" or self.mode != "search":
            return
        self._open_highlighted_hit()

    def _set_mode(self, mode: str) -> None:
        self.mode = mode
        bar = self.query_one("#status")
        label = self.query_one("#mode", Static)
        inp = self.query_one("#cmd", CommandInput)
        bar.remove_class("search", "para", "insert")
        label.update(MODE_LABELS.get(mode, mode.upper()))
        if mode == "search":
            bar.add_class("search")
            if self.search_nav:
                inp.blur()
                return
            inp.focus()
            inp.action_end()
            return
        if mode == "para":
            bar.add_class(mode)
            inp.focus()
            return
        inp.blur()
        self.query_one("#scroll", ScrollableContainer).focus()

    def _lock_search(self) -> None:
        if not self.hits:
            self._leave_search()
            return
        self.search_nav = True
        self._set_mode("search")

    def _close_filter(self) -> None:
        self.query_one("#results", OptionList).display = False
        self.query_one("#scroll").display = True

    def _render_results(self, query: str) -> None:
        results = self.query_one("#results", OptionList)
        keep = None
        if (
            self.hits
            and results.highlighted is not None
            and 0 <= results.highlighted < len(self.hits)
        ):
            keep = self.hits[results.highlighted].norm.citation
        results.clear_options()
        if not self.law or not query.strip():
            self.hits = []
            return
        self.hits = search_norms(self.law, query)
        options: list[Option | None] = []
        highlight = 0
        for index, hit in enumerate(self.hits):
            if options:
                options.append(None)
            prompt = format_search_hit(hit, query, self.law.abbreviation)
            law_index = self.law.norms.index(hit.norm)
            options.append(Option(prompt, id=f"norm-{law_index}"))
            if keep and hit.norm.citation == keep:
                highlight = index
        if options:
            results.add_options(options)
            results.highlighted = highlight

    def _open_highlighted_hit(self) -> None:
        if self.mode != "search" or self.law is None:
            return
        results = self.query_one("#results", OptionList)
        index = results.highlighted
        hits = list(self.hits)
        if index is None or not (0 <= index < len(hits)):
            self._leave_search()
            return
        self.hit_index = index
        self._jump_to_norm(hits[index].norm)

    def _leave_search(self) -> None:
        self.search_nav = False
        self._close_filter()
        self._set_mode("normal")
        self.query_one("#cmd", CommandInput).value = ""

    def _jump_to_norm(self, norm: Norm) -> None:
        self._leave_search()
        self._select_norm(norm)

    def _run_query(self, raw: str) -> None:
        if self.law is None:
            return
        query = raw.strip()
        if not query:
            return
        if query.startswith("/"):
            self.action_enter_search()
            needle = query[1:]
            self.query_one("#cmd", CommandInput).value = needle
            self._render_results(needle)
            return
        norm = lookup_norm(self.law, query)
        if norm is None:
            return
        self.hits = []
        self.hit_index = -1
        self._select_norm(norm)

    def _select_norm(self, norm: Norm, *, pin_top: bool = True) -> None:
        if self.law is None:
            return
        for index, item in enumerate(self.law.norms):
            if item is norm or item.citation == norm.citation:
                self._select_index(index, pin_top=pin_top)
                return

    def _select_index(self, index: int, *, pin_top: bool = False) -> None:
        if self.law is None or not (0 <= index < len(self.law.norms)):
            return
        self._current_index = index
        self.current = self.law.norms[index]
        self._pending_pin_top = pin_top
        self._update_pos()
        self._remember_position()
        block = self._block_at(index)
        if block is not None:
            self._mark_current(block)
            return
        if not self.query(NormBlock):
            self._fill_window(index)
            block = self._block_at(index)
            if block is not None:
                self._mark_current(block)
            return
        self._remount_around(index)

    def _block_at(self, index: int) -> NormBlock | None:
        for block in self.query(NormBlock):
            if block.norm_index == index:
                return block
        return None

    def _mark_current(self, block: NormBlock) -> None:
        same = self._current_block is block
        if not same:
            with self.app.batch_update():
                if self._current_block is not None and self._current_block.is_mounted:
                    self._current_block.remove_class("-current")
                block.add_class("-current")
                self._current_block = block
        self.call_after_refresh(self._pin_current)

    def _fill_window(self, index: int) -> None:
        if self.law is None:
            return
        start, end = window_range(index, len(self.law.norms))
        scroll = self.query_one("#scroll", ScrollableContainer)
        scroll.mount(
            *(
                NormBlock(self.law.norms[i], i, self.law.abbreviation)
                for i in range(start, end)
            )
        )
        self._mounted = (start, end)

    @work(exclusive=True, group="norm-window")
    async def _remount_around(self, index: int) -> None:
        if self.law is None:
            return
        start, end = window_range(index, len(self.law.norms))
        scroll = self.query_one("#scroll", ScrollableContainer)
        self._current_block = None
        async with scroll.batch():
            await scroll.remove_children()
            await scroll.mount(
                *(
                    NormBlock(self.law.norms[i], i, self.law.abbreviation)
                    for i in range(start, end)
                )
            )
        self._mounted = (start, end)
        block = self._block_at(index)
        if block is not None:
            self._mark_current(block)

    def _pin_current(self) -> None:
        block = self._current_block
        if block is None:
            return
        scroll = self.query_one("#scroll", ScrollableContainer)
        if not scroll.display:
            self._schedule_pin()
            return
        if not block.virtual_region_with_margin.size:
            self._schedule_pin()
            return
        scroll.scroll_to_widget(
            block,
            animate=False,
            top=self._pending_pin_top,
            immediate=True,
        )
        self._pin_retries = 0

    def _schedule_pin(self) -> None:
        if self._pin_retries >= 8:
            self._pin_retries = 0
            return
        self._pin_retries += 1
        self.call_after_refresh(self._pin_current)

    def _update_pos(self) -> None:
        label = self.query_one("#pos", Static)
        if self.law is None or not self.law.norms or self.current is None:
            label.update("")
            return
        label.update(law_pos_label(self.law, self.current))

    def _remember_position(self) -> None:
        if self.current is not None:
            self.session.set(self.ref.slug, self.current.citation)

    def _step_norm(self, delta: int) -> None:
        if self.law is None or not self.law.norms:
            return
        if self._current_index < 0:
            index = 0 if delta > 0 else len(self.law.norms) - 1
        else:
            index = self._current_index + delta
            if index < 0 or index >= len(self.law.norms):
                return
        self._select_index(index)

    def _step_hit(self, delta: int) -> None:
        if not self.hits:
            return
        self.hit_index = (max(self.hit_index, 0) + delta) % len(self.hits)
        self._select_norm(self.hits[self.hit_index].norm)


class NormenApp(App[None]):
    TITLE = "normen"
    SUB_TITLE = ""
    ENABLE_COMMAND_PALETTE = False
    CSS = """
    Screen {
        background: #10171e;
        color: #ced4df;
    }

    Header {
        background: #131a21;
        color: #ced4df;
        text-style: bold;
    }

    HeaderIcon, HeaderClockSpace {
        display: none;
        width: 0;
        padding: 0;
    }

    HeaderTitle {
        content-align: left middle;
        padding: 0 1;
        width: 1fr;
    }

    #cmd {
        border: none;
        background: #131a21;
        color: #ced4df;
        padding: 0 1;
        height: 1;
    }

    #cmd:focus {
        border: none;
    }

    #status {
        dock: bottom;
        height: 1;
        padding: 0 1;
        background: #131a21;
        color: #9ce5c0;
        text-style: bold;
    }

    #status.insert, #status.search, #status.para {
        color: #10171e;
        background: #f5d595;
    }

    #mode {
        width: auto;
    }

    #pos {
        width: 1fr;
        content-align: right middle;
        text-align: right;
    }

    #picker-body, #scroll, #results {
        height: 1fr;
    }

    OptionList {
        background: #10171e;
        border: none;
        padding: 1 1;
        height: 1fr;
    }

    #results {
        padding: 1 1 0 1;
        scrollbar-size-vertical: 1;
        scrollbar-background: #10171e;
        scrollbar-color: #2a3138;
        scrollbar-color-hover: #40474e;
        scrollbar-color-active: #ced4df;
    }

    #results > .option-list--option {
        background: #131a21;
        color: #ced4df;
        padding: 1 1;
    }

    #results > .option-list--option-highlighted,
    #results:focus > .option-list--option-highlighted {
        background: #2a3138;
        color: #ced4df;
        text-style: none;
    }

    #results > .option-list--option-hover {
        background: #131a21;
    }

    #results > .option-list--separator {
        color: #10171e;
        background: #10171e;
    }

    OptionList > .option-list--option-highlighted {
        background: #2a3138;
        color: #ced4df;
        text-style: none;
    }

    #scroll {
        padding: 1 1 0 1;
        background: #10171e;
        scrollbar-size-vertical: 1;
        scrollbar-background: #10171e;
        scrollbar-color: #2a3138;
        scrollbar-color-hover: #40474e;
        scrollbar-color-active: #ced4df;
    }

    NormBlock {
        background: #131a21;
        color: #ced4df;
        height: auto;
        margin: 0 0 1 0;
        padding: 1 1;
        border-left: wide #131a21;
    }

    NormBlock .heading {
        text-style: bold;
        color: #ced4df;
        padding-bottom: 1;
    }

    NormBlock.section .heading {
        color: #a3b8ef;
    }

    NormBlock .body {
        color: #ced4df;
        height: auto;
    }

    NormBlock .spaced {
        padding-top: 1;
    }

    NormBlock .nr-item {
        width: 1fr;
        height: auto;
        padding-left: 1;
    }

    NormBlock .nr-mark {
        width: auto;
        min-width: 3;
        padding-right: 1;
        color: #ced4df;
        height: auto;
    }

    NormBlock .nr-body {
        width: 1fr;
        height: auto;
        color: #ced4df;
    }

    NormBlock.-current {
        background: #131a21;
        border-left: wide #9ce5c0;
    }
    """

    def __init__(
        self,
        initial_law: str | None = None,
        initial_norm: str | None = None,
        refresh: bool = False,
        library: LawLibrary | None = None,
        session: SessionStore | None = None,
        config: Config | None = None,
    ) -> None:
        super().__init__()
        self.initial_law = initial_law
        self.initial_norm = initial_norm
        self.refresh_remote = refresh
        self.library = library or LawLibrary()
        self.session = session or SessionStore()
        self.config = config or Config()

    def get_default_screen(self) -> Screen:
        return PickerScreen()

    def on_mount(self) -> None:
        self.set_keymap(self.config.keymap())
        self.register_theme(PASTEL_DARK)
        self.theme = "pastel-dark"
        if not self.initial_law:
            return
        ref = resolve_law(self.initial_law)
        if ref is None:
            return
        self.push_screen(
            ReaderScreen(
                ref,
                self.library,
                self.session,
                self.refresh_remote,
                self.initial_norm,
            )
        )
