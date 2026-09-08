from __future__ import annotations

from rich.text import Text
from textual import events
from textual.app import App
from textual.binding import Binding
from textual.widgets import Static

from normen.catalog import LawRef, resolve_law
from normen.config import Config
from normen.fetch import LawLibrary
from normen.session import SessionStore
from normen.theme import PASTEL_DARK
from normen.tui.keys import MODE_LABELS, _event_matches, _is_yes_key
from normen.tui.picker import PickerScreen
from normen.tui.reader import ReaderScreen
from normen.tui.widgets import TabBar


class NormenApp(App[None]):
    TITLE = "normen"
    SUB_TITLE = ""
    ENABLE_COMMAND_PALETTE = False
    BINDINGS = [
        Binding("ctrl+n", "tab_prefix", "Tabs", id="tab_prefix", priority=True, show=False),
        Binding("ctrl+q", "quit_ask", "Quit", id="quit", priority=True, show=False),
    ]
    CSS = """
    Screen {
        background: #10171e;
        color: #ced4df;
    }

    TabBar, #tabs {
        background: #131a21;
        color: #ced4df;
        height: 1;
        padding: 0;
        width: 100%;
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

    #status.quit #pos {
        color: #ef8891;
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
        self.menu = PickerScreen()
        super().__init__()
        self.initial_law = initial_law
        self.initial_norm = initial_norm
        self.refresh_remote = refresh
        self.library = library or LawLibrary()
        self.session = session or SessionStore()
        self.config = config or Config()
        self.tabs: list[ReaderScreen] = []
        self.active = -1
        self.last = -1
        self._tab_seq = 0
        self._quit_pending = False
        self._prefix = False

    def get_default_screen(self) -> Screen:
        return self.menu

    def on_mount(self) -> None:
        self.set_keymap(self.config.keymap())
        self.register_theme(PASTEL_DARK)
        self.theme = "pastel-dark"
        if not self.initial_law:
            return
        ref = resolve_law(self.initial_law)
        if ref is None:
            return
        self.open_tab(ref, self.initial_norm)

    async def on_event(self, event: events.Event) -> None:
        if isinstance(event, events.Key) and self._consume_app_key(event):
            return
        await super().on_event(event)

    def _consume_app_key(self, event: events.Key) -> bool:
        if self._quit_pending:
            event.prevent_default()
            event.stop()
            if _is_yes_key(event):
                self.exit()
            else:
                self._cancel_quit()
            return True
        if event.key == "ctrl+q":
            event.prevent_default()
            event.stop()
            if self._prefix:
                self._disarm_prefix()
            self._ask_quit()
            return True
        keys = self.config.keymap()
        if self._prefix:
            event.prevent_default()
            event.stop()
            if _event_matches(event, keys.get("tab_prefix", "ctrl+n")):
                self._arm_prefix()
                return True
            self._run_prefix_suffix(event, keys)
            return True
        if _event_matches(event, keys.get("tab_prefix", "ctrl+n")):
            event.prevent_default()
            event.stop()
            self._arm_prefix()
            return True
        return False

    def action_tab_prefix(self) -> None:
        self._arm_prefix()

    def _arm_prefix(self) -> None:
        self._prefix = True
        try:
            self.screen.query_one("#mode", Static).update("PREFIX")
        except Exception:
            pass

    def _disarm_prefix(self) -> None:
        self._prefix = False
        self._restore_mode_label()

    def _restore_mode_label(self) -> None:
        screen = self.screen
        try:
            label = screen.query_one("#mode", Static)
        except Exception:
            return
        mode = getattr(screen, "mode", "normal")
        if mode == "insert":
            label.update("INSERT")
            return
        label.update(MODE_LABELS.get(mode, str(mode).upper()))

    def _run_prefix_suffix(self, event: events.Key, keys: dict[str, str]) -> None:
        self._disarm_prefix()
        if _event_matches(event, keys.get("tab_next", "n")):
            self.next_tab()
            return
        if _event_matches(event, keys.get("tab_prev", "p")):
            self.prev_tab()
            return
        if _event_matches(event, keys.get("tab_close", "x")):
            self.close_tab()
            return
        if _event_matches(event, keys.get("tab_menu", "m")) or event.key == "0":
            self.show_menu()
            return
        if event.key.isdigit():
            self.action_tab_jump(int(event.key))

    def action_quit_ask(self) -> None:
        self._ask_quit()

    def _ask_quit(self) -> None:
        self._quit_pending = True
        try:
            self.screen.query_one("#status").add_class("quit")
            self.screen.query_one("#pos", Static).update("Quit? (y)")
        except Exception:
            pass

    def _cancel_quit(self) -> None:
        self._quit_pending = False
        screen = self.screen
        try:
            screen.query_one("#status").remove_class("quit")
            if isinstance(screen, ReaderScreen):
                screen._update_pos()
            else:
                screen.query_one("#pos", Static).update("")
        except Exception:
            pass

    def action_tab_next(self) -> None:
        self.next_tab()

    def action_tab_prev(self) -> None:
        self.prev_tab()

    def action_tab_close(self) -> None:
        self.close_tab()

    def action_tab_menu(self) -> None:
        self.show_menu()

    def action_tab_jump(self, number: int) -> None:
        index = int(number)
        if index <= 0:
            self.show_menu()
            return
        self.switch_tab(index - 1)

    def tab_line(self) -> Text:
        line = Text()
        menu_marker = "*" if self.active < 0 else ("-" if self.last < 0 else "")
        menu_style = (
            "#131a21 on #9ce5c0" if self.active < 0 else "#ced4df on #40474e"
        )
        line.append(f" 0:MENU{menu_marker} ", menu_style)
        for index, tab in enumerate(self.tabs):
            line.append(" ")
            marker = "*" if index == self.active else ("-" if index == self.last else "")
            label = f" {index + 1}:{tab.ref.shortcut}{marker} "
            if index == self.active:
                line.append(label, style="#131a21 on #9ce5c0")
            else:
                line.append(label, style="#ced4df on #40474e")
        return line

    def refresh_tabs(self) -> None:
        line = self.tab_line()
        try:
            for bar in self.screen.query(TabBar):
                bar.display = True
                bar.update(line)
        except Exception:
            pass

    def show_menu(self) -> None:
        if self.screen is self.menu:
            self.refresh_tabs()
            return
        if self.active >= 0:
            self.last = self.active
        self.active = -1
        self.pop_screen()
        self.refresh_tabs()

    def _show_law(self, screen: ReaderScreen) -> None:
        if self.screen is screen:
            return
        if self.screen is self.menu:
            self.push_screen(screen)
            return
        self.switch_screen(screen)

    def open_tab(self, ref: LawRef, initial_query: str | None = None) -> None:
        screen = ReaderScreen(
            ref,
            self.library,
            self.session,
            self.refresh_remote,
            initial_query,
        )
        name = f"tab-{self._tab_seq}"
        self._tab_seq += 1
        self.install_screen(screen, name)
        self.last = self.active
        self.tabs.append(screen)
        self.active = len(self.tabs) - 1
        self._show_law(screen)
        self.refresh_tabs()

    def switch_tab(self, index: int) -> None:
        if not (0 <= index < len(self.tabs)):
            self.refresh_tabs()
            return
        target = self.tabs[index]
        if self.screen is target:
            self.refresh_tabs()
            return
        if index != self.active:
            self.last = self.active
            self.active = index
        self._show_law(target)
        self.refresh_tabs()

    def _goto_slot(self, slot: int) -> None:
        if slot <= 0:
            self.show_menu()
            return
        self.switch_tab(slot - 1)

    def next_tab(self) -> None:
        slots = 1 + len(self.tabs)
        current = 0 if self.active < 0 else self.active + 1
        self._goto_slot((current + 1) % slots)

    def prev_tab(self) -> None:
        slots = 1 + len(self.tabs)
        current = 0 if self.active < 0 else self.active + 1
        self._goto_slot((current - 1) % slots)

    def close_tab(self) -> None:
        if self.active < 0 or not self.tabs:
            self.refresh_tabs()
            return
        closing_i = self.active
        closing = self.tabs[closing_i]
        showing = self.screen is closing
        go_menu = self.last < 0 or len(self.tabs) == 1
        next_i = -1
        if not go_menu and self.last != closing_i and 0 <= self.last < len(self.tabs):
            next_i = self.last - (1 if self.last > closing_i else 0)
        elif not go_menu:
            next_i = closing_i if closing_i < len(self.tabs) - 1 else closing_i - 1
        self.tabs.pop(closing_i)
        if go_menu or next_i < 0:
            self.active = -1
            self.last = -1
            if showing:
                self.pop_screen()
        else:
            self.active = next_i
            self.last = -1
            if showing:
                self._show_law(self.tabs[next_i])
        if self.is_screen_installed(closing):
            self.uninstall_screen(closing)
        self.refresh_tabs()
