from __future__ import annotations

from textual.app import ComposeResult
from textual.binding import Binding
from textual.containers import Vertical
from textual.screen import Screen
from textual.widgets import Input, OptionList, Static

from normen.catalog import LawRef, filter_laws, resolve_law
from normen.tui.keys import INSERT_ONLY, SEARCH_EDIT_ACTIONS, SEARCH_NAV_ACTIONS
from normen.tui.widgets import CommandInput, TabBar, _law_option, _status_bar


class PickerScreen(Screen):
    BINDINGS = [
        Binding("i", "enter_insert", "Insert", id="enter_insert", priority=True),
        Binding("slash", "enter_search", "Suche", id="enter_search", priority=True),
        Binding("escape", "enter_normal", "Normal", show=False, id="enter_normal", priority=True),
        Binding("h,left", "move_left", "H", id="move_left", priority=True),
        Binding("j,down", "move_down", "J", id="move_down", priority=True),
        Binding("k,up", "move_up", "K", id="move_up", priority=True),
        Binding("l,right", "move_right", "L", id="move_right", priority=True),
        Binding("enter", "confirm", "Öffnen", id="confirm", priority=True),
    ]

    def __init__(self) -> None:
        super().__init__()
        self.mode = "normal"
        self.search_nav = False

    @property
    def filtering(self) -> bool:
        return self.mode == "search"

    def on_mount(self) -> None:
        self.title = "normen"
        self.sub_title = ""
        self._render_laws("")
        self._set_mode("normal")

    def compose(self) -> ComposeResult:
        yield TabBar()
        yield CommandInput(placeholder="", id="cmd")
        with Vertical(id="picker-body"):
            yield OptionList(id="laws")
        yield from _status_bar()

    def on_screen_resume(self) -> None:
        refresh = getattr(self.app, "refresh_tabs", None)
        if callable(refresh):
            refresh()

    def check_action(self, action: str, parameters: tuple) -> bool | None:
        if self.mode == "insert" and action not in INSERT_ONLY:
            return False
        if self.mode == "search":
            allowed = SEARCH_NAV_ACTIONS if self.search_nav else SEARCH_EDIT_ACTIONS
            return action in allowed
        return True

    def action_enter_insert(self) -> None:
        self._set_mode("insert")

    def action_enter_search(self) -> None:
        if self.mode == "search":
            self.search_nav = False
            self._set_mode("search")
            return
        self.search_nav = False
        self.query_one("#cmd", CommandInput).value = ""
        self._render_laws("")
        self._set_mode("search")

    def action_enter_normal(self) -> None:
        self.search_nav = False
        self.query_one("#cmd", CommandInput).value = ""
        self._render_laws("")
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
        if self.mode == "search" and self.search_nav:
            self._open_highlighted()
            return
        if self.mode != "search":
            self._open_highlighted()

    def on_input_changed(self, event: Input.Changed) -> None:
        if self.mode != "search" or self.search_nav:
            return
        self._render_laws(event.value.lstrip("/"))

    def on_input_submitted(self, event: Input.Submitted) -> None:
        if self.mode == "search" and not self.search_nav:
            self._lock_search()
            return
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
        bar.remove_class("insert", "search")
        if mode == "insert":
            bar.add_class("insert")
            label.update("INSERT")
            inp.focus()
            return
        if mode == "search":
            bar.add_class("search")
            label.update("SEARCH")
            if self.search_nav:
                inp.blur()
                self.query_one("#laws", OptionList).focus()
                return
            inp.focus()
            inp.action_end()
            return
        label.update("NORMAL")
        inp.blur()
        self.query_one("#laws", OptionList).focus()

    def _lock_search(self) -> None:
        query = self.query_one("#cmd", CommandInput).value.lstrip("/")
        if not filter_laws(query):
            self.action_enter_normal()
            return
        self.search_nav = True
        self._set_mode("search")

    def _render_laws(self, query: str) -> None:
        law_list = self.query_one("#laws", OptionList)
        keep = None
        if law_list.option_count and law_list.highlighted is not None:
            keep = law_list.get_option_at_index(law_list.highlighted).id
        law_list.clear_options()
        refs = filter_laws(query)
        if not refs:
            return
        highlight = 0
        options = []
        for index, ref in enumerate(refs):
            options.append(_law_option(ref, query))
            if keep and ref.slug == keep:
                highlight = index
        law_list.add_options(options)
        law_list.highlighted = highlight

    def _open_highlighted(self) -> None:
        law_list = self.query_one("#laws", OptionList)
        if law_list.highlighted is None:
            return
        option = law_list.get_option_at_index(law_list.highlighted)
        ref = resolve_law(option.id or "")
        if ref is not None:
            self._open(ref)

    def _open(self, ref: LawRef) -> None:
        self.search_nav = False
        self.query_one("#cmd", CommandInput).value = ""
        self._render_laws("")
        self._set_mode("normal")
        open_tab = getattr(self.app, "open_tab", None)
        if callable(open_tab):
            open_tab(ref)
