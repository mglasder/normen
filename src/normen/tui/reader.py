from __future__ import annotations

from textual import events, work
from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import ScrollableContainer
from textual.screen import Screen
from textual.widgets import Input, OptionList, Static
from textual.widgets.option_list import Option

from normen.catalog import LawRef
from normen.document import law_pos_label
from normen.fetch import LawLibrary
from normen.models import Law, Norm, SearchHit
from normen.search import (
    format_search_hit,
    lookup_norm,
    parse_citation_query,
    search_norms,
)
from normen.session import SessionStore
from normen.tui.keys import (
    MODE_LABELS,
    SEARCH_EDIT_ACTIONS,
    SEARCH_NAV_ACTIONS,
    TYPING_ONLY,
    _para_char,
    window_range,
)
from normen.tui.widgets import CommandInput, NormBlock, TabBar, _law_title, _status_bar


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
        yield TabBar()
        yield CommandInput(placeholder="", id="cmd")
        with ScrollableContainer(id="scroll"):
            yield Static(
                f"Lade {self.ref.shortcut} von gesetze-im-internet.de …", id="loading"
            )
        yield OptionList(id="results")
        yield from _status_bar()

    def on_screen_resume(self) -> None:
        refresh = getattr(self.app, "refresh_tabs", None)
        if callable(refresh):
            refresh()

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
        self.on_para_key(event)

    def on_para_key(self, event: events.Key) -> None:
        if self.mode != "para":
            return
        if event.key == "backspace":
            event.stop()
            self._para = self._para[:-1]
            self._sync_para()
            return
        char = event.character
        if (
            char
            and _para_char(char)
            and not char.isdigit()
            and any(part.isdigit() for part in self._para)
        ):
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
        if self.mode == "normal":
            scroll.focus()
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
        inp = self.query_one("#cmd", CommandInput)
        inp.value = self._para
        inp.cursor_position = len(self._para)

    def action_move_down(self) -> None:
        if self.mode == "search":
            self._step_result(1)
            return
        self.query_one("#scroll", ScrollableContainer).scroll_down(
            animate=False, immediate=True
        )
        self._sync_current_to_viewport()

    def action_move_up(self) -> None:
        if self.mode == "search":
            self._step_result(-1)
            return
        self.query_one("#scroll", ScrollableContainer).scroll_up(
            animate=False, immediate=True
        )
        self._sync_current_to_viewport()

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
        self.query_one("#scroll", ScrollableContainer).scroll_page_down(
            animate=False, immediate=True
        )
        self._sync_current_to_viewport()

    def action_page_up(self) -> None:
        self.query_one("#scroll", ScrollableContainer).scroll_page_up(
            animate=False, immediate=True
        )
        self._sync_current_to_viewport()

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
            inp.action_end()
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
        exact = lookup_norm(self.law, query) if parse_citation_query(query) else None
        options: list[Option | None] = []
        highlight = 0
        for index, hit in enumerate(self.hits):
            if options:
                options.append(None)
            prompt = format_search_hit(hit, query, self.law.abbreviation)
            law_index = self.law.norms.index(hit.norm)
            options.append(Option(prompt, id=f"norm-{law_index}"))
            if exact is not None:
                if hit.norm.citation == exact.citation:
                    highlight = index
            elif keep and hit.norm.citation == keep:
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

    def _mark_current(self, block: NormBlock, *, pin: bool = True) -> None:
        same = self._current_block is block
        if not same:
            with self.app.batch_update():
                if self._current_block is not None and self._current_block.is_mounted:
                    self._current_block.remove_class("-current")
                block.add_class("-current")
                self._current_block = block
        if pin:
            self.call_after_refresh(self._pin_current)

    def _adopt_index(self, index: int) -> None:
        if self.law is None or not (0 <= index < len(self.law.norms)):
            return
        self._current_index = index
        self.current = self.law.norms[index]
        self._update_pos()
        self._remember_position()
        block = self._block_at(index)
        if block is not None:
            self._mark_current(block, pin=False)

    def _sync_current_to_viewport(self) -> None:
        if self.law is None:
            return
        scroll = self.query_one("#scroll", ScrollableContainer)
        top = scroll.scroll_offset.y
        passed: NormBlock | None = None
        for block in sorted(self.query(NormBlock), key=lambda item: item.norm_index):
            if block.virtual_region.y <= top:
                passed = block
            elif passed is not None:
                break
        if passed is None:
            return
        if passed.norm_index == self._current_index:
            return
        self._adopt_index(passed.norm_index)

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


