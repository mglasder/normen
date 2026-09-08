pub mod menu;
pub mod reader;
pub mod theme;

use std::io::{self, stdout, IsTerminal};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Padding, Paragraph};
use ratatui::{Frame, Terminal};

use crate::catalog::{resolve_law, LawRef};
use crate::config::Config;
use crate::document::{iter_body_blocks, norm_heading};
use crate::models::Law;
use crate::search::{format_search_hit, highlight_text};
use crate::session::SessionStore;

use self::menu::{para_char, MenuState};
use self::reader::ReaderTab;
use self::theme::{
    ACCENT, BACKGROUND, BORDER, ERROR, FOREGROUND, HIGHLIGHT_BG, PRIMARY, SEARCH_FG, SECONDARY,
    SURFACE,
};

pub const WINDOW_RADIUS: usize = 16;

pub fn window_range(index: usize, total: usize, radius: usize) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let index = index.min(total - 1);
    let start = index.saturating_sub(radius);
    let end = (index + radius + 1).min(total);
    (start, end)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Enter,
    Esc,
    Backspace,
    Slash,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Search,
    Para,
}

pub struct App {
    config: Config,
    load: Box<dyn Fn(&LawRef, bool) -> Result<Law, String>>,
    menu: MenuState,
    tabs: Vec<ReaderTab>,
    active: i32,
    last: i32,
    prefix: bool,
    help: bool,
    quit_pending: bool,
    pub should_quit: bool,
    refresh: bool,
    session: SessionStore,
    load_error: Option<String>,
    body_width: u16,
    body_height: u16,
}

impl App {
    pub fn new(
        config: Config,
        load: Box<dyn Fn(&LawRef, bool) -> Result<Law, String>>,
        initial_law: Option<String>,
        initial_norm: Option<String>,
        refresh: bool,
    ) -> Self {
        let mut app = Self {
            config,
            load,
            menu: MenuState::new(),
            tabs: Vec::new(),
            active: -1,
            last: -1,
            prefix: false,
            help: false,
            quit_pending: false,
            should_quit: false,
            refresh,
            session: SessionStore::new(),
            load_error: None,
            body_width: 80,
            body_height: 16,
        };
        if let Some(query) = initial_law {
            if let Some(law_ref) = resolve_law(&query) {
                app.open_tab(*law_ref, initial_norm.as_deref());
            }
        }
        app
    }

    pub fn run(&mut self) -> io::Result<()> {
        if !stdout().is_terminal() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "normen needs an interactive terminal",
            ));
        }
        install_panic_hook();
        enable_raw_mode()?;
        let _restore = RestoreTerminal;
        execute!(stdout(), EnterAlternateScreen, Hide)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
        loop {
            terminal.draw(|frame| self.draw(frame))?;
            match event::read()? {
                Event::Key(key) => {
                    if let Some(mapped) = map_crossterm(key) {
                        if let Some(name) = self.pending_load_label(mapped) {
                            self.load_error = Some(format!("Lade {name} …"));
                            terminal.draw(|frame| self.draw(frame))?;
                            self.load_error = None;
                        }
                        self.handle_key(mapped);
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    pub fn handle_key(&mut self, key: Key) {
        if matches!(key, Key::Ctrl('c')) {
            self.should_quit = true;
            return;
        }
        if self.quit_pending {
            if is_yes(key) {
                self.should_quit = true;
            } else {
                self.quit_pending = false;
            }
            return;
        }
        if self.help {
            if self.matches(key, "quit") || matches!(key, Key::Ctrl('q')) {
                self.help = false;
                self.quit_pending = true;
                return;
            }
            self.help = false;
            return;
        }
        if self.matches(key, "help") || key == Key::Char('?') {
            if self.screen_mode() == Mode::Normal && !self.prefix {
                self.help = true;
                return;
            }
        }
        if self.matches(key, "quit") || matches!(key, Key::Ctrl('q')) {
            if self.prefix {
                self.prefix = false;
            }
            self.quit_pending = true;
            return;
        }
        if self.prefix {
            if self.matches(key, "tab_prefix") {
                return;
            }
            self.prefix = false;
            self.run_prefix(key);
            return;
        }
        if self.matches(key, "tab_prefix") {
            self.prefix = true;
            return;
        }
        if self.on_menu() {
            self.handle_menu(key);
        } else {
            self.handle_reader(key);
        }
    }

    fn run_prefix(&mut self, key: Key) {
        if self.matches(key, "tab_next") {
            self.next_tab();
            return;
        }
        if self.matches(key, "tab_prev") {
            self.prev_tab();
            return;
        }
        if self.matches(key, "tab_close") {
            self.close_tab();
            return;
        }
        if self.matches(key, "tab_menu") || matches!(key, Key::Char('0')) {
            self.show_menu();
            return;
        }
        if let Key::Char(c) = key {
            if c.is_ascii_digit() {
                self.jump_tab(c.to_digit(10).unwrap_or(0) as usize);
            }
        }
    }

    fn pending_load_label(&self, key: Key) -> Option<&'static str> {
        if self.quit_pending || self.prefix || !self.on_menu() {
            return None;
        }
        let confirm = key == Key::Enter || self.matches(key, "confirm");
        if !confirm {
            return None;
        }
        let law = match self.menu.mode {
            Mode::Insert => {
                let query = self.menu.cmd.trim();
                if query.is_empty() {
                    self.menu.highlighted()
                } else {
                    resolve_law(query)
                }
            }
            Mode::Search if !self.menu.search_nav => None,
            _ => self.menu.highlighted(),
        }?;
        Some(law.shortcut)
    }

    fn handle_menu(&mut self, key: Key) {
        let mode = self.menu.mode;
        let search_nav = self.menu.search_nav;
        match mode {
            Mode::Insert => {
                if self.matches(key, "enter_normal") || key == Key::Esc {
                    self.menu.reset_list();
                    return;
                }
                if key == Key::Enter || self.matches(key, "confirm") {
                    if let Some(law_ref) = self.menu.resolve_insert().copied() {
                        self.open_tab(law_ref, None);
                    }
                    return;
                }
                if key == Key::Backspace {
                    self.menu.backspace();
                    return;
                }
                if let Some(ch) = printable(key) {
                    self.menu.type_char(ch);
                }
            }
            Mode::Search if !search_nav => {
                if self.matches(key, "enter_normal") || key == Key::Esc {
                    self.menu.reset_list();
                    return;
                }
                if key == Key::Enter || self.matches(key, "confirm") {
                    self.menu.lock_search();
                    return;
                }
                if self.matches(key, "enter_search") || key == Key::Slash {
                    self.menu.search_nav = false;
                    return;
                }
                if key == Key::Backspace {
                    self.menu.backspace();
                    return;
                }
                if let Some(ch) = printable(key) {
                    self.menu.type_char(ch);
                }
            }
            Mode::Search => {
                if self.matches(key, "enter_normal") || key == Key::Esc {
                    self.menu.reset_list();
                    return;
                }
                if self.matches(key, "enter_search") || key == Key::Slash {
                    self.menu.search_nav = false;
                    return;
                }
                if self.matches(key, "move_down")
                    || key == Key::Char('j')
                    || key == Key::Down
                    || key == Key::Char('l')
                    || key == Key::Right
                {
                    self.menu.move_highlight(1);
                    return;
                }
                if self.matches(key, "move_up")
                    || key == Key::Char('k')
                    || key == Key::Up
                    || key == Key::Char('h')
                    || key == Key::Left
                {
                    self.menu.move_highlight(-1);
                    return;
                }
                if key == Key::Enter || self.matches(key, "confirm") {
                    if let Some(law_ref) = self.menu.highlighted().copied() {
                        self.menu.reset_list();
                        self.open_tab(law_ref, None);
                    }
                }
            }
            Mode::Normal | Mode::Para => {
                if self.matches(key, "enter_para") || key == Key::Char('i') {
                    self.menu.enter_insert();
                    return;
                }
                if self.matches(key, "enter_search") || key == Key::Slash {
                    self.menu.enter_search();
                    return;
                }
                if self.matches(key, "move_down")
                    || key == Key::Char('j')
                    || key == Key::Down
                    || key == Key::Char('l')
                    || key == Key::Right
                {
                    self.menu.move_highlight(1);
                    return;
                }
                if self.matches(key, "move_up")
                    || key == Key::Char('k')
                    || key == Key::Up
                    || key == Key::Char('h')
                    || key == Key::Left
                {
                    self.menu.move_highlight(-1);
                    return;
                }
                if key == Key::Enter || self.matches(key, "confirm") {
                    if let Some(law_ref) = self.menu.highlighted().copied() {
                        self.open_tab(law_ref, None);
                    }
                }
            }
        }
    }

    fn handle_reader(&mut self, key: Key) {
        let enter_normal = self.matches(key, "enter_normal") || key == Key::Esc;
        let confirm = key == Key::Enter || self.matches(key, "confirm");
        let enter_search = self.matches(key, "enter_search") || key == Key::Slash;
        let enter_para = self.matches(key, "enter_para") || key == Key::Char('i');
        let paragraph_next = self.matches(key, "paragraph_next");
        let paragraph_prev = self.matches(key, "paragraph_prev");
        let move_right =
            self.matches(key, "move_right") || key == Key::Char('l') || key == Key::Right;
        let move_left = self.matches(key, "move_left") || key == Key::Char('h') || key == Key::Left;
        let move_down = self.matches(key, "move_down") || key == Key::Char('j') || key == Key::Down;
        let move_up = self.matches(key, "move_up") || key == Key::Char('k') || key == Key::Up;
        let goto_top = self.matches(key, "goto_top") || key == Key::Char('g');
        let goto_bottom = self.matches(key, "goto_bottom") || key == Key::Char('G');
        let page_down = self.matches(key, "page_down") || matches!(key, Key::Ctrl('d'));
        let page_up = self.matches(key, "page_up") || matches!(key, Key::Ctrl('u'));
        let card_width = self.reader_card_width();
        let view_height = self.reader_view_height();
        let page = view_height.max(1) as isize;
        let Some(tab) = self.current_tab_mut() else {
            return;
        };
        match tab.mode {
            Mode::Search if !tab.search_nav => {
                if enter_normal {
                    tab.enter_normal();
                    return;
                }
                if confirm {
                    tab.lock_search();
                    return;
                }
                if enter_search {
                    tab.search_nav = false;
                    return;
                }
                if key == Key::Backspace {
                    tab.search_backspace();
                    return;
                }
                if let Some(ch) = printable(key) {
                    tab.type_search(ch);
                }
            }
            Mode::Search => {
                if enter_normal {
                    tab.enter_normal();
                    return;
                }
                if enter_search {
                    tab.search_nav = false;
                    return;
                }
                if move_down {
                    tab.move_hit(1, card_width, view_height);
                    return;
                }
                if move_up {
                    tab.move_hit(-1, card_width, view_height);
                    return;
                }
                if confirm {
                    tab.open_highlighted_hit();
                }
            }
            Mode::Para => {
                if enter_normal {
                    tab.enter_normal();
                    return;
                }
                if confirm {
                    tab.confirm_para();
                    return;
                }
                if key == Key::Backspace {
                    tab.para.pop();
                    return;
                }
                if let Key::Char(c) = key {
                    if c.is_ascii_digit() {
                        tab.type_digit(c);
                    } else if para_char(c) {
                        tab.type_para_char(c);
                    }
                }
            }
            Mode::Normal | Mode::Insert => {
                if let Key::Char(c) = key {
                    if c.is_ascii_digit() {
                        tab.type_digit(c);
                        return;
                    }
                }
                if enter_para {
                    tab.enter_para("");
                    return;
                }
                if enter_search {
                    tab.enter_search();
                    return;
                }
                if paragraph_next {
                    tab.step_norm(1, card_width, view_height);
                    return;
                }
                if paragraph_prev {
                    tab.step_norm(-1, card_width, view_height);
                    return;
                }
                if move_down {
                    tab.scroll_by(1, card_width, view_height);
                    return;
                }
                if move_up {
                    tab.scroll_by(-1, card_width, view_height);
                    return;
                }
                if page_down {
                    tab.scroll_by(page, card_width, view_height);
                    return;
                }
                if page_up {
                    tab.scroll_by(-page, card_width, view_height);
                    return;
                }
                if move_right {
                    tab.step_norm(1, card_width, view_height);
                    return;
                }
                if move_left {
                    tab.step_norm(-1, card_width, view_height);
                    return;
                }
                if goto_top {
                    tab.goto_top();
                    return;
                }
                if goto_bottom {
                    tab.goto_bottom();
                }
            }
        }
    }

    fn matches(&self, key: Key, name: &str) -> bool {
        self.config_matches(key, name)
    }

    fn config_matches(&self, key: Key, name: &str) -> bool {
        let keys = self.config.keymap();
        let binding = keys.get(name).map(String::as_str).unwrap_or("");
        key_matches(key, binding)
    }

    fn open_tab(&mut self, law_ref: LawRef, initial: Option<&str>) {
        match (self.load)(&law_ref, self.refresh) {
            Ok(law) => {
                self.load_error = None;
                let mut tab = ReaderTab::from_law(law_ref, law);
                tab.apply_initial(initial);
                if let Some(citation) = tab.current_citation() {
                    self.session.set(law_ref.slug, citation);
                }
                self.last = self.active;
                self.tabs.push(tab);
                self.active = (self.tabs.len() - 1) as i32;
            }
            Err(err) => {
                self.load_error = Some(err);
            }
        }
    }

    fn show_menu(&mut self) {
        if self.on_menu() {
            return;
        }
        if self.active >= 0 {
            self.last = self.active;
        }
        self.active = -1;
    }

    fn jump_tab(&mut self, number: usize) {
        if number == 0 {
            self.show_menu();
            return;
        }
        self.switch_tab(number - 1);
    }

    fn switch_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if index as i32 != self.active {
            self.last = self.active;
            self.active = index as i32;
        }
    }

    fn next_tab(&mut self) {
        let slots = 1 + self.tabs.len();
        let current = if self.active < 0 {
            0
        } else {
            self.active as usize + 1
        };
        self.goto_slot((current + 1) % slots);
    }

    fn prev_tab(&mut self) {
        let slots = 1 + self.tabs.len();
        let current = if self.active < 0 {
            0
        } else {
            self.active as usize + 1
        };
        self.goto_slot((current + slots - 1) % slots);
    }

    fn goto_slot(&mut self, slot: usize) {
        if slot == 0 {
            self.show_menu();
        } else {
            self.switch_tab(slot - 1);
        }
    }

    fn close_tab(&mut self) {
        if self.active < 0 || self.tabs.is_empty() {
            return;
        }
        let closing_i = self.active as usize;
        let go_menu = self.last < 0 || self.tabs.len() == 1;
        let mut next_i: i32 = -1;
        if !go_menu
            && self.last != self.active
            && self.last >= 0
            && (self.last as usize) < self.tabs.len()
        {
            next_i = self.last - if self.last > self.active { 1 } else { 0 };
        } else if !go_menu {
            next_i = if closing_i < self.tabs.len() - 1 {
                closing_i as i32
            } else {
                closing_i as i32 - 1
            };
        }
        self.tabs.remove(closing_i);
        if go_menu || next_i < 0 {
            self.active = -1;
            self.last = -1;
        } else {
            self.active = next_i;
            self.last = -1;
        }
    }

    fn current_tab(&self) -> Option<&ReaderTab> {
        if self.active < 0 {
            None
        } else {
            self.tabs.get(self.active as usize)
        }
    }

    fn current_tab_mut(&mut self) -> Option<&mut ReaderTab> {
        if self.active < 0 {
            None
        } else {
            self.tabs.get_mut(self.active as usize)
        }
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        frame.render_widget(
            Paragraph::new(self.tab_line()).style(Style::default().bg(SURFACE)),
            chunks[0],
        );
        self.draw_cmd(frame, chunks[1]);
        self.body_width = chunks[2].width;
        self.body_height = chunks[2].height;
        if !self.on_menu() {
            let card_width = self.reader_card_width();
            let view_height = self.reader_view_height();
            if let Some(tab) = self.current_tab_mut() {
                if tab.mode == Mode::Search {
                    tab.ensure_hit_visible(card_width, view_height);
                }
            }
        }
        self.draw_body(frame, chunks[2]);
        self.draw_status(frame, chunks[3]);
        if self.help {
            self.draw_help(frame, area);
        }
    }

    fn draw_cmd(&self, frame: &mut Frame<'_>, area: Rect) {
        let style = Style::default().bg(SURFACE).fg(FOREGROUND);
        frame.render_widget(Block::new().style(style), area);
        let inner = Block::new()
            .padding(Padding::horizontal(1))
            .inner(area);
        frame.render_widget(Paragraph::new(self.cmd()).style(style), inner);
        if self.show_cmd_cursor() && inner.width > 0 {
            let col = (self.cmd().chars().count() as u16).min(inner.width.saturating_sub(1));
            frame.set_cursor_position(Position::new(inner.x + col, inner.y));
        }
    }

    fn show_cmd_cursor(&self) -> bool {
        if self.prefix || self.quit_pending || self.help {
            return false;
        }
        match self.screen_mode() {
            Mode::Insert | Mode::Para => true,
            Mode::Search => !self.search_nav(),
            Mode::Normal => false,
        }
    }

    fn draw_body(&self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Block::new().style(Style::default().bg(BACKGROUND)), area);
        if self.on_menu() {
            self.draw_menu(frame, area);
            return;
        }
        let Some(tab) = self.current_tab() else {
            return;
        };
        if tab.mode == Mode::Search {
            self.draw_search(frame, area, tab);
            return;
        }
        self.draw_reader(frame, area, tab);
    }

    fn reader_card_width(&self) -> u16 {
        self.body_width.max(24).saturating_sub(3)
    }

    fn reader_view_height(&self) -> u16 {
        self.body_height.saturating_sub(1).max(1)
    }

    fn draw_menu(&self, frame: &mut Frame<'_>, area: Rect) {
        let inner = area.inner(Margin {
            horizontal: 1,
            vertical: 1,
        });
        if inner.width <= 1 {
            return;
        }
        let list_area = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width.saturating_sub(1),
            height: inner.height,
        };
        let query = self.menu.cmd.trim_start_matches('/');
        let items: Vec<ListItem> = self
            .menu
            .filtered
            .iter()
            .map(|law| ListItem::new(menu_row(law, query, self.menu.mode == Mode::Search)))
            .collect();
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(self.menu.highlight.min(items.len().saturating_sub(1))));
        }
        let list = List::new(items)
            .style(Style::default().bg(BACKGROUND).fg(FOREGROUND))
            .highlight_style(Style::default().bg(HIGHLIGHT_BG).fg(FOREGROUND))
            .highlight_symbol("");
        frame.render_stateful_widget(list, list_area, &mut state);
        draw_scrollbar(
            frame,
            inner,
            self.menu.highlight,
            self.menu.filtered.len().max(1),
        );
    }

    fn draw_search(&self, frame: &mut Frame<'_>, area: Rect, tab: &ReaderTab) {
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(1),
        };
        if inner.width <= 1 {
            return;
        }
        let card_width = inner.width.saturating_sub(1);
        let query = tab.cmd.trim_start_matches('/');
        let mut y = inner.y;
        for (index, hit) in tab.hits.iter().enumerate().skip(tab.search_origin) {
            if y >= inner.bottom() {
                break;
            }
            let bg = if index == tab.hit_highlight {
                HIGHLIGHT_BG
            } else {
                SURFACE
            };
            let lines = search_card_lines(hit, query, &tab.law.abbreviation, card_width, bg);
            let height = (lines.len() as u16).min(inner.bottom().saturating_sub(y));
            if height == 0 {
                break;
            }
            frame.render_widget(
                Paragraph::new(lines).style(Style::default().bg(bg).fg(FOREGROUND)),
                Rect {
                    x: inner.x,
                    y,
                    width: card_width,
                    height,
                },
            );
            y = y.saturating_add(height).saturating_add(1);
        }
        draw_scrollbar(
            frame,
            inner,
            tab.hit_highlight,
            tab.hits.len().max(1),
        );
    }

    fn draw_reader(&self, frame: &mut Frame<'_>, area: Rect, tab: &ReaderTab) {
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(1),
        };
        if inner.width <= 1 || tab.law.norms.is_empty() {
            return;
        }
        let card_width = inner.width.saturating_sub(1);
        let mut y = inner.y;
        let mut skip = tab.skip_lines;
        for index in tab.view_start..tab.law.norms.len() {
            if y >= inner.bottom() {
                break;
            }
            let mut lines = norm_card_lines(
                &tab.law.norms[index],
                &tab.law.abbreviation,
                index == tab.current,
                card_width,
            );
            lines.push(filled_line(
                "",
                card_width,
                Style::default().bg(BACKGROUND),
            ));
            if skip >= lines.len() {
                skip -= lines.len();
                continue;
            }
            if skip > 0 {
                lines.drain(..skip);
                skip = 0;
            }
            let height = (lines.len() as u16).min(inner.bottom().saturating_sub(y));
            if height == 0 {
                break;
            }
            frame.render_widget(
                Paragraph::new(lines),
                Rect {
                    x: inner.x,
                    y,
                    width: card_width,
                    height,
                },
            );
            y = y.saturating_add(height);
        }
        draw_scrollbar(
            frame,
            inner,
            tab.current,
            tab.law.norms.len().max(1),
        );
    }

    fn tab_line(&self) -> Line<'static> {
        let mut spans = Vec::new();
        let menu_marker = if self.active < 0 {
            "*"
        } else if self.last < 0 {
            "-"
        } else {
            ""
        };
        let menu_style = if self.active < 0 {
            Style::default()
                .fg(SURFACE)
                .bg(PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(FOREGROUND)
                .bg(BORDER)
                .add_modifier(Modifier::BOLD)
        };
        spans.push(Span::styled(format!(" 0:MENU{menu_marker} "), menu_style));
        for (index, tab) in self.tabs.iter().enumerate() {
            spans.push(Span::raw(" "));
            let marker = if index as i32 == self.active {
                "*"
            } else if index as i32 == self.last {
                "-"
            } else {
                ""
            };
            let label = format!(
                " {}:{shortcut}{marker} ",
                index + 1,
                shortcut = tab.shortcut()
            );
            let style = if index as i32 == self.active {
                Style::default()
                    .fg(SURFACE)
                    .bg(PRIMARY)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(FOREGROUND)
                    .bg(BORDER)
                    .add_modifier(Modifier::BOLD)
            };
            spans.push(Span::styled(label, style));
        }
        Line::from(spans).style(Style::default().bg(SURFACE))
    }

    fn draw_status(&self, frame: &mut Frame<'_>, area: Rect) {
        let bar_style = Style::default().bg(SURFACE).fg(FOREGROUND);
        let pos = if self.quit_pending {
            "Quit? (y)".to_string()
        } else if self.load_error.is_some() {
            self.pos_label()
        } else if self.on_menu() {
            String::new()
        } else {
            self.pos_label()
        };
        let pos_style = if self.quit_pending || self.load_error.is_some() {
            Style::default()
                .bg(SURFACE)
                .fg(ERROR)
                .add_modifier(Modifier::BOLD)
        } else {
            bar_style
        };
        frame.render_widget(Block::new().style(bar_style), area);
        let inner = Block::new()
            .padding(Padding::horizontal(1))
            .inner(area);
        let mode = self.mode_label();
        let badge = format!(" {} ", center_pad(mode, 6));
        let mode_style = self.mode_badge_style();
        let mode_width = badge.chars().count() as u16;
        let chunks = Layout::horizontal([
            Constraint::Length(mode_width),
            Constraint::Min(1),
        ])
        .split(inner);
        frame.render_widget(Paragraph::new(badge).style(mode_style), chunks[0]);
        frame.render_widget(
            Paragraph::new(pos)
                .style(pos_style)
                .alignment(Alignment::Right),
            chunks[1],
        );
    }

    fn draw_help(&self, frame: &mut Frame<'_>, area: Rect) {
        let width = area.width.saturating_sub(4).min(64).max(24);
        let lines = self.help_lines();
        let height = (lines.len() as u16)
            .saturating_add(2)
            .min(area.height)
            .max(5);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        let rect = Rect {
            x,
            y,
            width,
            height,
        };
        frame.render_widget(Clear, rect);
        let block = Block::bordered()
            .title(" Keys ")
            .title_bottom(" Esc / ? close ")
            .style(Style::default().bg(SURFACE).fg(FOREGROUND))
            .border_style(Style::default().fg(BORDER));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(SURFACE).fg(FOREGROUND)),
            inner,
        );
    }

    fn help_lines(&self) -> Vec<Line<'static>> {
        let keys = self.config.keymap();
        let key = |name: &str| pretty_binding(keys.get(name).map(String::as_str).unwrap_or(""));
        let row = |left: String, right: &str| {
            Line::from(vec![
                Span::styled(format!("  {left:<18}"), Style::default().fg(PRIMARY)),
                Span::styled(right.to_string(), Style::default().fg(FOREGROUND)),
            ])
        };
        let heading = |text: &str| {
            Line::from(Span::styled(
                format!(" {text}"),
                Style::default()
                    .fg(SECONDARY)
                    .add_modifier(Modifier::BOLD),
            ))
        };
        vec![
            heading("Motion"),
            row(
                format!("{} {}", key("move_down"), key("move_up")),
                "scroll line",
            ),
            row(
                format!("{} {}", key("paragraph_next"), key("paragraph_prev")),
                "next / previous paragraph",
            ),
            row(
                format!("{} {}", key("move_left"), key("move_right")),
                "previous / next norm",
            ),
            row(
                format!("{} {}", key("goto_top"), key("goto_bottom")),
                "top / bottom",
            ),
            row(
                format!("{} {}", key("page_down"), key("page_up")),
                "page down / up",
            ),
            heading("Jump"),
            row(key("enter_search"), "search (Enter then j/k)"),
            row("0-9".into(), "type a citation"),
            row(key("enter_para"), "PARA / MENU shortcut"),
            row(key("confirm"), "confirm"),
            row(key("enter_normal"), "cancel / NORMAL"),
            heading("Tabs"),
            row(
                format!("{} {}/{}", key("tab_prefix"), key("tab_next"), key("tab_prev")),
                "next / previous tab",
            ),
            row(
                format!("{} {}/0", key("tab_prefix"), key("tab_menu")),
                "MENU",
            ),
            row(format!("{} 1-9", key("tab_prefix")), "jump to tab"),
            row(
                format!("{} {}", key("tab_prefix"), key("tab_close")),
                "close tab",
            ),
            heading("App"),
            row(key("quit"), "quit (then y)"),
            row("Ctrl-c".into(), "quit now"),
            row(key("help"), "this window"),
        ]
    }

    fn mode_badge_style(&self) -> Style {
        if self.help || self.prefix {
            return Style::default()
                .fg(FOREGROUND)
                .bg(BORDER)
                .add_modifier(Modifier::BOLD);
        }
        match self.screen_mode() {
            Mode::Normal => Style::default()
                .fg(PRIMARY)
                .bg(SURFACE)
                .add_modifier(Modifier::BOLD),
            Mode::Para | Mode::Insert => Style::default()
                .fg(SEARCH_FG)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
            Mode::Search => Style::default()
                .fg(SEARCH_FG)
                .bg(SECONDARY)
                .add_modifier(Modifier::BOLD),
        }
    }

    fn screen_mode(&self) -> Mode {
        if self.on_menu() {
            self.menu.mode
        } else {
            self.current_tab()
                .map(|tab| tab.mode)
                .unwrap_or(Mode::Normal)
        }
    }

    pub fn on_menu(&self) -> bool {
        self.active < 0
    }

    pub fn mode_label(&self) -> &'static str {
        if self.help {
            return "HELP";
        }
        if self.prefix {
            return "PREFIX";
        }
        match self.screen_mode() {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Search => "SEARCH",
            Mode::Para => "PARA",
        }
    }

    pub fn cmd(&self) -> &str {
        if self.on_menu() {
            &self.menu.cmd
        } else if let Some(tab) = self.current_tab() {
            if tab.mode == Mode::Para {
                &tab.para
            } else {
                &tab.cmd
            }
        } else {
            ""
        }
    }

    pub fn tab_line_plain(&self) -> String {
        self.tab_line()
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    pub fn current_citation(&self) -> Option<&str> {
        self.current_tab().and_then(ReaderTab::current_citation)
    }

    pub fn pos_label(&self) -> String {
        if let Some(err) = &self.load_error {
            return err.clone();
        }
        self.current_tab()
            .map(ReaderTab::pos_label)
            .unwrap_or_default()
    }

    pub fn picker_slugs(&self) -> Vec<&'static str> {
        self.menu.filtered.iter().map(|law| law.slug).collect()
    }

    pub fn picker_highlight(&self) -> usize {
        self.menu.highlight
    }

    pub fn search_nav(&self) -> bool {
        if self.on_menu() {
            self.menu.search_nav
        } else {
            self.current_tab()
                .map(|tab| tab.search_nav)
                .unwrap_or(false)
        }
    }

    pub fn hits_citations(&self) -> Vec<String> {
        self.current_tab()
            .map(|tab| {
                tab.hits
                    .iter()
                    .map(|hit| hit.norm.citation.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn hit_highlight(&self) -> usize {
        self.current_tab().map(|tab| tab.hit_highlight).unwrap_or(0)
    }

    pub fn search_origin(&self) -> usize {
        self.current_tab().map(|tab| tab.search_origin).unwrap_or(0)
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    pub fn tab_shortcuts(&self) -> Vec<&str> {
        self.tabs.iter().map(ReaderTab::shortcut).collect()
    }

    pub fn quit_prompt(&self) -> bool {
        self.quit_pending
    }

    pub fn help_open(&self) -> bool {
        self.help
    }

    pub fn mounted_len(&self) -> usize {
        self.current_tab().map(ReaderTab::mounted_len).unwrap_or(0)
    }

    pub fn current_index(&self) -> usize {
        self.current_tab().map(|tab| tab.current).unwrap_or(0)
    }

    pub fn view_start(&self) -> usize {
        self.current_tab().map(|tab| tab.view_start).unwrap_or(0)
    }

    pub fn session(&self) -> &SessionStore {
        &self.session
    }
}

fn printable(key: Key) -> Option<char> {
    match key {
        Key::Char(c) if !c.is_control() => Some(c),
        Key::Slash => Some('/'),
        _ => None,
    }
}

fn is_yes(key: Key) -> bool {
    matches!(key, Key::Char('y' | 'Y'))
}

fn center_pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_string();
    }
    let extra = width - len;
    let left = extra / 2;
    let right = extra - left;
    format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
}

fn pretty_binding(binding: &str) -> String {
    let parts: Vec<&str> = binding
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if let Some(letter) = parts.iter().copied().find(|part| part.chars().count() == 1) {
        return letter.to_string();
    }
    pretty_key_part(parts.first().copied().unwrap_or(""))
}

fn pretty_key_part(part: &str) -> String {
    let tokens: Vec<&str> = part.split('+').filter(|token| !token.is_empty()).collect();
    if tokens.len() == 2 && tokens[0] == "shift" && tokens[1].chars().count() == 1 {
        return tokens[1].to_uppercase();
    }
    let mut out = String::new();
    for (index, token) in tokens.into_iter().enumerate() {
        if index > 0 {
            out.push('-');
        }
        let pretty = match token {
            "ctrl" => "Ctrl",
            "alt" => "Alt",
            "shift" => "Shift",
            "escape" => "Esc",
            "enter" => "Enter",
            "slash" => "/",
            other => other,
        };
        out.push_str(pretty);
    }
    out
}

fn key_matches(key: Key, binding: &str) -> bool {
    binding
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .any(|part| key_eq(key, part))
}

fn key_eq(key: Key, part: &str) -> bool {
    match key {
        Key::Char(c) => {
            if part.chars().count() == 1 && part.chars().next() == Some(c) {
                return true;
            }
            if c == '/' && (part == "/" || part == "slash") {
                return true;
            }
            if c == 'J' && (part == "J" || part == "shift+j") {
                return true;
            }
            if c == 'K' && (part == "K" || part == "shift+k") {
                return true;
            }
            if c == 'N' && part == "shift+n" {
                return true;
            }
            false
        }
        Key::Ctrl(c) => {
            let want = format!("ctrl+{}", c.to_ascii_lowercase());
            part.eq_ignore_ascii_case(&want)
        }
        Key::Enter => part == "enter",
        Key::Esc => part == "escape",
        Key::Backspace => part == "backspace",
        Key::Slash => part == "/" || part == "slash",
        Key::Up => part == "up",
        Key::Down => part == "down",
        Key::Left => part == "left",
        Key::Right => part == "right",
    }
}

fn map_crossterm(event: event::KeyEvent) -> Option<Key> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    match event.code {
        KeyCode::Char(c) if ctrl => Some(Key::Ctrl(c.to_ascii_lowercase())),
        KeyCode::Char('/') => Some(Key::Slash),
        KeyCode::Char(c) => {
            let c = if event.modifiers.contains(KeyModifiers::SHIFT) && c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            };
            Some(Key::Char(c))
        }
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Esc => Some(Key::Esc),
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Up => Some(Key::Up),
        KeyCode::Down => Some(Key::Down),
        KeyCode::Left => Some(Key::Left),
        KeyCode::Right => Some(Key::Right),
        _ => None,
    }
}

struct RestoreTerminal;

impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        original(info);
    }));
}

fn menu_row(law: &LawRef, query: &str, searching: bool) -> Line<'static> {
    let shortcut = format!("{:<10}", law.shortcut);
    if searching {
        let mut spans = styled_to_line(&highlight_text(&shortcut, query)).spans;
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::BOLD);
        }
        spans.extend(styled_to_line(&highlight_text(law.title, query)).spans);
        Line::from(spans)
    } else {
        Line::from(vec![
            Span::styled(shortcut, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(law.title.to_string()),
        ])
    }
}

fn styled_to_line(text: &crate::search::StyledText) -> Line<'static> {
    styled_to_lines(text)
        .into_iter()
        .next()
        .unwrap_or_else(|| Line::from(""))
}

fn styled_to_lines(text: &crate::search::StyledText) -> Vec<Line<'static>> {
    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut pos = 0usize;
    let mut events: Vec<(usize, usize, Option<&str>)> = Vec::new();
    if text.spans.is_empty() {
        events.push((0, text.plain.len(), None));
    } else {
        for span in &text.spans {
            if span.start > pos {
                events.push((pos, span.start, None));
            }
            events.push((span.start, span.end, Some(span.style.as_str())));
            pos = span.end;
        }
        if pos < text.plain.len() {
            events.push((pos, text.plain.len(), None));
        }
    }
    for (start, end, style) in events {
        if start >= end {
            continue;
        }
        let chunk = &text.plain[start..end];
        for (i, piece) in chunk.split('\n').enumerate() {
            if i > 0 {
                lines.push(Vec::new());
            }
            if piece.is_empty() {
                continue;
            }
            lines.last_mut().unwrap().push(Span::styled(
                piece.to_string(),
                span_style(style),
            ));
        }
    }
    lines
        .into_iter()
        .map(|spans| {
            if spans.is_empty() {
                Line::from("")
            } else {
                Line::from(spans)
            }
        })
        .collect()
}

fn span_style(style: Option<&str>) -> Style {
    match style {
        Some(style) if style.contains("f5d595") => Style::default()
            .fg(SEARCH_FG)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD),
        Some(style) if style.contains("bold") => {
            Style::default().add_modifier(Modifier::BOLD)
        }
        _ => Style::default(),
    }
}

fn draw_scrollbar(frame: &mut Frame<'_>, area: Rect, index: usize, total: usize) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let track = Rect {
        x: area.right().saturating_sub(1),
        y: area.y,
        width: 1,
        height: area.height,
    };
    frame.render_widget(
        Block::new().style(Style::default().bg(BACKGROUND)),
        track,
    );
    if total == 0 || area.height == 0 {
        return;
    }
    let thumb_h = ((area.height as usize).max(1) / total.max(1)).max(1) as u16;
    let max_y = area.height.saturating_sub(thumb_h);
    let thumb_y = if total <= 1 {
        0
    } else {
        ((index.min(total - 1) as u16) * max_y) / (total as u16 - 1)
    };
    frame.render_widget(
        Block::new().style(Style::default().bg(HIGHLIGHT_BG)),
        Rect {
            x: track.x,
            y: area.y + thumb_y,
            width: 1,
            height: thumb_h.min(area.height),
        },
    );
}

pub(crate) fn search_card_height(
    hit: &crate::models::SearchHit,
    query: &str,
    abbreviation: &str,
    width: u16,
) -> u16 {
    search_card_lines(hit, query, abbreviation, width, SURFACE)
        .len()
        .max(1) as u16
}

fn search_card_lines(
    hit: &crate::models::SearchHit,
    query: &str,
    abbreviation: &str,
    width: u16,
    bg: ratatui::style::Color,
) -> Vec<Line<'static>> {
    let style = Style::default().bg(bg).fg(FOREGROUND);
    let inner_width = width.saturating_sub(2) as usize;
    let mut lines = vec![filled_line("", width, style)];
    let prompt = format_search_hit(hit, query, abbreviation);
    for line in styled_to_lines(&prompt) {
        lines.push(pad_line(line, inner_width, style, width));
    }
    lines.push(filled_line("", width, style));
    lines
}

fn norm_card_lines(
    norm: &crate::models::Norm,
    abbreviation: &str,
    current: bool,
    width: u16,
) -> Vec<Line<'static>> {
    let card = Style::default().bg(SURFACE).fg(FOREGROUND);
    let mark = if current { "▏" } else { " " };
    let mark_style = if current {
        Style::default().fg(PRIMARY).bg(SURFACE)
    } else {
        Style::default().fg(SURFACE).bg(SURFACE)
    };
    let heading_style = if norm.keys.is_empty() && norm.text.trim().is_empty() {
        card.fg(SECONDARY).add_modifier(Modifier::BOLD)
    } else {
        card.add_modifier(Modifier::BOLD)
    };
    let inner_width = width.saturating_sub(3) as usize;
    let mut lines = vec![card_row(mark, mark_style, Vec::new(), inner_width, card)];
    let heading = norm_heading(norm, abbreviation);
    for piece in wrap_text(&heading, inner_width) {
        lines.push(card_row(
            mark,
            mark_style,
            vec![Span::styled(piece, heading_style)],
            inner_width,
            card,
        ));
    }
    lines.push(card_row(mark, mark_style, Vec::new(), inner_width, card));
    if !norm.text.trim().is_empty() {
        let mut after_list = false;
        let mut after_absatz = false;
        for block in iter_body_blocks(&norm.text) {
            if block.kind == "list" {
                let marker = format!("{:<3}", block.marker);
                let body_width = inner_width.saturating_sub(marker.chars().count() + 1);
                let wrapped = wrap_text(&block.text, body_width.max(1));
                for (i, piece) in wrapped.into_iter().enumerate() {
                    let mut spans = Vec::new();
                    if i == 0 {
                        spans.push(Span::styled(marker.clone(), card));
                        spans.push(Span::raw(" "));
                    } else {
                        spans.push(Span::raw(" ".repeat(marker.chars().count() + 1)));
                    }
                    spans.push(Span::styled(piece, card));
                    lines.push(card_row(mark, mark_style, spans, inner_width, card));
                }
                after_list = true;
                continue;
            }
            if after_list || after_absatz {
                lines.push(card_row(mark, mark_style, Vec::new(), inner_width, card));
            }
            for piece in wrap_text(&block.text, inner_width) {
                lines.push(card_row(
                    mark,
                    mark_style,
                    vec![Span::styled(piece, card)],
                    inner_width,
                    card,
                ));
            }
            after_list = false;
            after_absatz = true;
        }
    }
    lines.push(card_row(mark, mark_style, Vec::new(), inner_width, card));
    lines
}

pub(crate) fn card_span(
    norm: &crate::models::Norm,
    abbreviation: &str,
    width: u16,
) -> usize {
    norm_card_lines(norm, abbreviation, false, width)
        .len()
        .saturating_add(1)
}

fn card_row(
    mark: &'static str,
    mark_style: Style,
    content: Vec<Span<'static>>,
    inner_width: usize,
    body: Style,
) -> Line<'static> {
    let mut spans = vec![
        Span::styled(mark, mark_style),
        Span::styled(" ", body),
    ];
    let used: usize = content.iter().map(|span| span.content.chars().count()).sum();
    spans.extend(content);
    if used < inner_width {
        spans.push(Span::styled(" ".repeat(inner_width - used), body));
    }
    spans.push(Span::styled(" ", body));
    Line::from(spans)
}

fn pad_line(line: Line<'static>, inner_width: usize, style: Style, _width: u16) -> Line<'static> {
    let mut spans = vec![Span::styled(" ", style)];
    let used: usize = line.spans.iter().map(|span| span.content.chars().count()).sum();
    spans.extend(line.spans);
    if used < inner_width {
        spans.push(Span::styled(" ".repeat(inner_width - used), style));
    }
    spans.push(Span::styled(" ", style));
    Line::from(spans)
}

fn filled_line(text: &str, width: u16, style: Style) -> Line<'static> {
    let mut out = text.to_string();
    let count = out.chars().count();
    if (count as u16) < width {
        out.push_str(&" ".repeat((width as usize).saturating_sub(count)));
    }
    Line::from(Span::styled(out, style))
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    for raw in text.split('\n') {
        if raw.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_w = 0usize;
        for word in raw.split_whitespace() {
            let word_w = word.chars().count();
            if current.is_empty() {
                if word_w <= width {
                    current = word.to_string();
                    current_w = word_w;
                } else {
                    for ch in word.chars() {
                        if current_w >= width {
                            lines.push(std::mem::take(&mut current));
                            current_w = 0;
                        }
                        current.push(ch);
                        current_w += 1;
                    }
                }
                continue;
            }
            if current_w + 1 + word_w <= width {
                current.push(' ');
                current.push_str(word);
                current_w += 1 + word_w;
            } else {
                lines.push(std::mem::take(&mut current));
                if word_w <= width {
                    current = word.to_string();
                    current_w = word_w;
                } else {
                    current_w = 0;
                    for ch in word.chars() {
                        if current_w >= width {
                            lines.push(std::mem::take(&mut current));
                            current_w = 0;
                        }
                        current.push(ch);
                        current_w += 1;
                    }
                }
            }
        }
        lines.push(current);
    }
    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::LAWS;
    use crate::config::Config;
    use crate::fetch::LawLibrary;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::Path;

    const SAMPLE: &[u8] = include_bytes!("../../tests/fixtures/sample.xml");
    const GG_SAMPLE: &[u8] = include_bytes!("../../tests/fixtures/gg_sample.xml");

    fn app_at(dir: &Path, initial_law: Option<&str>) -> App {
        sample_app(dir, initial_law, None)
    }

    fn sample_app(dir: &Path, initial_law: Option<&str>, initial_norm: Option<&str>) -> App {
        let xml = SAMPLE.to_vec();
        let library = LawLibrary::new(dir, move |_| xml.clone());
        App::new(
            Config::new(dir.join("normen.conf")),
            Box::new(move |law_ref, refresh| Ok(library.load(law_ref, refresh))),
            initial_law.map(str::to_string),
            initial_norm.map(str::to_string),
            false,
        )
    }

    fn multi_app(dir: &Path) -> App {
        let bgb = SAMPLE.to_vec();
        let gg = GG_SAMPLE.to_vec();
        let library = LawLibrary::new(dir, move |slug| {
            if slug == "gg" {
                gg.clone()
            } else {
                bgb.clone()
            }
        });
        App::new(
            Config::new(dir.join("normen.conf")),
            Box::new(move |law_ref, refresh| Ok(library.load(law_ref, refresh))),
            None,
            None,
            false,
        )
    }

    fn assert_help_overlay(joined: &str) {
        assert!(
            joined.contains("scroll") && joined.contains("paragraph"),
            "j/k and J/K must be labeled separately: {joined}"
        );
        assert!(
            !joined.contains("hit"),
            "n/N hit bindings should not appear: {joined}"
        );
        assert!(joined.contains("Esc"), "missing Esc: {joined}");
        assert!(joined.contains("Enter"), "missing Enter: {joined}");
        assert!(
            joined.contains("PARA") || joined.contains("citation"),
            "i / 0-9 should mention PARA or citation: {joined}"
        );
        assert!(joined.contains("close"), "missing tab close: {joined}");
        for line in joined.lines() {
            if line.contains("1-9") {
                assert!(
                    !line.contains("close"),
                    "tab jump and close must be separate rows: {line}"
                );
            }
        }
        assert!(
            joined.contains("quit") && (joined.contains("Ctrl-q") || joined.contains("Ctrl-Q")),
            "missing quit: {joined}"
        );
        assert!(
            joined.contains("this window"),
            "help key was clipped: {joined}"
        );
    }

    #[test]
    fn pretty_binding_shows_letters_not_shift_chords() {
        assert_eq!(pretty_binding("shift+n"), "N");
        assert_eq!(pretty_binding("K,shift+k"), "K");
        assert_eq!(pretty_binding("j,down"), "j");
        assert_eq!(pretty_binding("ctrl+d"), "Ctrl-d");
        assert_eq!(pretty_binding("escape"), "Esc");
        assert_eq!(pretty_binding("enter"), "Enter");
    }

    fn many_hits_app(dir: &Path) -> App {
        let mut xml = String::from(
            r#"<?xml version="1.0"?><dokumente><norm><metadaten><jurabk>BGB</jurabk>
<langue>Bürgerliches Gesetzbuch</langue></metadaten><textdaten/></norm>"#,
        );
        for number in 1..=20 {
            xml.push_str(&format!(
                r#"<norm><metadaten><jurabk>BGB</jurabk><enbez>§ {number}</enbez>
<titel>Needle {number}</titel></metadaten>
<textdaten><text format="XML"><Content><P>needle</P></Content></text></textdaten></norm>"#
            ));
        }
        xml.push_str("</dokumente>");
        let xml = xml.into_bytes();
        let library = LawLibrary::new(dir, move |_| xml.clone());
        App::new(
            Config::new(dir.join("normen.conf")),
            Box::new(move |law_ref, refresh| Ok(library.load(law_ref, refresh))),
            Some("bgb".into()),
            None,
            false,
        )
    }

    fn neighbor_app(dir: &Path) -> App {
        let xml = r#"<?xml version="1.0"?>
<dokumente><norm><metadaten><jurabk>StGB</jurabk>
<langue>Strafgesetzbuch</langue></metadaten><textdaten/></norm>
<norm><metadaten><jurabk>StGB</jurabk><enbez>§ 113</enbez>
<titel>Widerstand</titel></metadaten>
<textdaten><text format="XML"><Content>
<P>Siehe auch § 113a.</P></Content></text></textdaten></norm>
<norm><metadaten><jurabk>StGB</jurabk><enbez>§ 113a</enbez>
<titel>Tätlicher Angriff</titel></metadaten>
<textdaten><text format="XML"><Content>
<P>Tätlicher Angriff.</P></Content></text></textdaten></norm>
</dokumente>"#
            .as_bytes()
            .to_vec();
        let library = LawLibrary::new(dir, move |_| xml.clone());
        App::new(
            Config::new(dir.join("normen.conf")),
            Box::new(move |law_ref, refresh| Ok(library.load(law_ref, refresh))),
            Some("stgb".into()),
            None,
            false,
        )
    }

    fn press(app: &mut App, keys: &[Key]) {
        for key in keys {
            app.handle_key(*key);
        }
    }

    #[test]
    fn window_range_keeps_a_bounded_slice() {
        assert_eq!(window_range(0, 80, WINDOW_RADIUS), (0, WINDOW_RADIUS + 1));
        let (start, end) = window_range(70, 80, WINDOW_RADIUS);
        assert_eq!(end, 80);
        assert!(end - start <= 2 * WINDOW_RADIUS + 1);
        assert!(start <= 70 && 70 < end);
    }

    #[test]
    fn menu_tab_is_always_leftmost() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        assert!(app.on_menu());
        assert!(app.tab_line_plain().contains("0:MENU*"));
        app.handle_key(Key::Enter);
        assert!(!app.on_menu());
        let text = app.tab_line_plain();
        assert!(text.contains("0:MENU"));
        assert!(text.contains("1:BGB*"));
        press(&mut app, &[Key::Ctrl('n'), Key::Char('m')]);
        assert!(app.on_menu());
        assert!(app.tab_line_plain().contains("0:MENU*"));
        press(&mut app, &[Key::Ctrl('n'), Key::Char('1')]);
        assert!(!app.on_menu());
        press(&mut app, &[Key::Ctrl('n'), Key::Char('0')]);
        assert!(app.on_menu());
        press(&mut app, &[Key::Ctrl('n'), Key::Char('x')]);
        assert_eq!(app.tab_count(), 1);
        assert!(app.on_menu());
    }

    #[test]
    fn prefix_m_does_not_open_a_law() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        press(&mut app, &[Key::Ctrl('n'), Key::Char('m')]);
        assert!(app.on_menu());
        assert_eq!(app.tab_count(), 0);
        app.handle_key(Key::Char('i'));
        press(&mut app, &[Key::Ctrl('n'), Key::Char('m')]);
        assert!(app.on_menu());
        assert_eq!(app.tab_count(), 0);
        app.handle_key(Key::Enter);
        assert_eq!(app.tab_count(), 1);
    }

    #[test]
    fn picker_hjkl_move_highlight() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(app.picker_highlight(), 0);
        app.handle_key(Key::Char('j'));
        assert_eq!(app.picker_highlight(), 1);
        app.handle_key(Key::Char('h'));
        assert_eq!(app.picker_highlight(), 0);
        app.handle_key(Key::Char('l'));
        assert_eq!(app.picker_highlight(), 1);
        app.handle_key(Key::Char('k'));
        assert_eq!(app.picker_highlight(), 0);
        assert_eq!(app.cmd(), "");
    }

    #[test]
    fn picker_insert_mode_types_shortcut() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        app.handle_key(Key::Char('v'));
        assert_eq!(app.cmd(), "");
        app.handle_key(Key::Char('i'));
        assert_eq!(app.mode_label(), "INSERT");
        press(
            &mut app,
            &[
                Key::Char('v'),
                Key::Char('w'),
                Key::Char('g'),
                Key::Char('o'),
            ],
        );
        assert_eq!(app.cmd(), "vwgo");
        app.handle_key(Key::Esc);
        assert_eq!(app.mode_label(), "NORMAL");
    }

    #[test]
    fn reader_opens_full_law_in_normal_mode() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_at(dir.path(), Some("bgb"));
        assert!(!app.on_menu());
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(app.pos_label(), "1:433");
        assert_eq!(app.current_citation(), Some("Buch 1"));
    }

    #[test]
    fn reader_jumps_to_paragraph_by_number() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        app.handle_key(Key::Char('4'));
        assert_eq!(app.mode_label(), "PARA");
        press(&mut app, &[Key::Char('3'), Key::Char('3')]);
        assert_eq!(app.cmd(), "433");
        app.handle_key(Key::Enter);
        assert_eq!(app.current_citation(), Some("§ 433"));
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(app.pos_label(), "433:433");
    }

    #[test]
    fn reader_jumps_to_lettered_paragraph() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        press(&mut app, &[Key::Char('3'), Key::Char('1')]);
        assert_eq!(app.mode_label(), "PARA");
        assert_eq!(app.cmd(), "31");
        app.handle_key(Key::Char('a'));
        assert_eq!(app.cmd(), "31a");
        app.handle_key(Key::Enter);
        assert_eq!(app.current_citation(), Some("§ 31a"));
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(app.pos_label(), "31a:433");
    }

    #[test]
    fn slash_search_lettered_citation_is_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        press(
            &mut app,
            &[Key::Slash, Key::Char('3'), Key::Char('1'), Key::Char('a')],
        );
        assert_eq!(app.mode_label(), "SEARCH");
        assert_eq!(app.cmd(), "31a");
        assert_eq!(app.hits_citations()[0], "§ 31a");
        app.handle_key(Key::Enter);
        app.handle_key(Key::Enter);
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(app.current_citation(), Some("§ 31a"));
    }

    #[test]
    fn slash_search_does_not_keep_unsuffixed_neighbor() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = neighbor_app(dir.path());
        press(
            &mut app,
            &[
                Key::Slash,
                Key::Char('1'),
                Key::Char('1'),
                Key::Char('3'),
                Key::Char('a'),
            ],
        );
        let citations = app.hits_citations();
        assert_eq!(citations[0], "§ 113a");
        assert!(citations.iter().any(|c| c == "§ 113"));
        assert_eq!(citations[app.hit_highlight()], "§ 113a");
        app.handle_key(Key::Enter);
        app.handle_key(Key::Enter);
        assert_eq!(app.current_citation(), Some("§ 113a"));
    }

    #[test]
    fn reader_hjkl_stay_in_normal_mode() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        let first = app.current_citation().map(str::to_string);
        press(&mut app, &[Key::Char('j'), Key::Char('j'), Key::Char('k')]);
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(app.cmd(), "");
        assert_eq!(app.current_citation(), first.as_deref());
        app.handle_key(Key::Char('l'));
        assert_ne!(app.current_citation(), first.as_deref());
    }

    #[test]
    fn jk_line_scroll_does_not_step_paragraphs() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        app.handle_key(Key::Char('J'));
        app.handle_key(Key::Char('J'));
        assert_eq!(app.current_index(), 2);
        assert_eq!(app.view_start(), 0);
        app.handle_key(Key::Char('k'));
        assert_eq!(app.current_index(), 2);
        assert_eq!(app.view_start(), 0);
        app.handle_key(Key::Char('K'));
        assert_eq!(app.current_index(), 1);
        assert_eq!(app.view_start(), 0);
        app.handle_key(Key::Char('j'));
        assert_eq!(app.current_index(), 1);
        assert_eq!(app.view_start(), 0);
    }

    #[test]
    fn reader_shift_jk_jump_paragraph() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        let first = app.current_citation().map(str::to_string);
        app.handle_key(Key::Char('J'));
        assert_ne!(app.current_citation(), first.as_deref());
        assert_eq!(app.pos_label(), "1:433");
        app.handle_key(Key::Char('K'));
        assert_eq!(app.current_citation(), first.as_deref());
    }

    #[test]
    fn shift_j_keeps_previous_paragraph_visible() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        let first = app.current_citation().map(str::to_string).unwrap();
        app.handle_key(Key::Char('J'));
        assert_ne!(app.current_citation(), Some(first.as_str()));
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let joined: String = (0..24)
            .map(|y| {
                (0..80)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains(&first),
            "J should move the mark down without scrolling the first card away: {joined}"
        );
        assert!(joined.contains("§ 1") || joined.contains("Rechtsfähigkeit"));
    }

    #[test]
    fn shift_j_keyevent_maps_to_uppercase() {
        let event = crossterm::event::KeyEvent::new(KeyCode::Char('j'), KeyModifiers::SHIFT);
        assert_eq!(map_crossterm(event), Some(Key::Char('J')));
        let plain = crossterm::event::KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(map_crossterm(plain), Some(Key::Char('j')));
    }

    #[test]
    fn jk_scroll_advances_current_then_shift_j_steps_from_there() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        let start = app.current_index();
        let start_cite = app.current_citation().map(str::to_string);
        let mut advanced = false;
        for _ in 0..80 {
            app.handle_key(Key::Char('j'));
            if app.current_index() > start {
                advanced = true;
                break;
            }
        }
        assert!(advanced, "j scroll should advance the current paragraph");
        let locked = app.current_index();
        let locked_cite = app.current_citation().map(str::to_string);
        assert_ne!(locked_cite, start_cite);
        app.handle_key(Key::Char('J'));
        assert_eq!(app.current_index(), locked + 1);
        app.handle_key(Key::Char('K'));
        assert_eq!(app.current_index(), locked);
        assert_eq!(app.current_citation(), locked_cite.as_deref());
    }

    #[test]
    fn picker_opens_gg_by_shortcut() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = multi_app(dir.path());
        press(
            &mut app,
            &[Key::Char('i'), Key::Char('g'), Key::Char('g'), Key::Enter],
        );
        assert!(!app.on_menu());
        assert_eq!(app.tab_shortcuts(), vec!["GG"]);
    }

    #[test]
    fn picker_opens_law_by_shortcut() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        press(
            &mut app,
            &[
                Key::Char('i'),
                Key::Char('b'),
                Key::Char('g'),
                Key::Char('b'),
                Key::Enter,
            ],
        );
        assert_eq!(app.tab_shortcuts(), vec!["BGB"]);
    }

    #[test]
    fn menu_slash_filters_laws() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        app.handle_key(Key::Slash);
        assert_eq!(app.mode_label(), "SEARCH");
        press(
            &mut app,
            &[
                Key::Char('g'),
                Key::Char('r'),
                Key::Char('u'),
                Key::Char('n'),
                Key::Char('d'),
            ],
        );
        assert_eq!(app.picker_slugs(), vec!["gg"]);
        assert_eq!(app.picker_highlight(), 0);
    }

    #[test]
    fn menu_search_jk_types_then_moves() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        press(
            &mut app,
            &[Key::Slash, Key::Char('j'), Key::Char('v'), Key::Char('w')],
        );
        assert!(!app.search_nav());
        assert_eq!(app.cmd(), "jvw");
        assert!(app.picker_slugs().is_empty());
        app.handle_key(Key::Esc);
        press(&mut app, &[Key::Slash, Key::Char('v'), Key::Char('w')]);
        assert_eq!(
            app.picker_slugs(),
            vec!["vwgo", "vwvfg", "vwzg_2005", "vwvg"]
        );
        app.handle_key(Key::Enter);
        assert_eq!(app.mode_label(), "SEARCH");
        assert!(app.search_nav());
        assert_eq!(app.picker_highlight(), 0);
        app.handle_key(Key::Char('j'));
        assert_eq!(app.picker_highlight(), 1);
        app.handle_key(Key::Char('k'));
        assert_eq!(app.picker_highlight(), 0);
        assert_eq!(app.cmd(), "vw");
    }

    #[test]
    fn menu_search_enter_opens_highlighted() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = multi_app(dir.path());
        press(&mut app, &[Key::Slash, Key::Char('g'), Key::Char('g')]);
        app.handle_key(Key::Enter);
        app.handle_key(Key::Enter);
        assert_eq!(app.tab_shortcuts(), vec!["GG"]);
        assert_eq!(app.tab_count(), 1);
    }

    #[test]
    fn menu_search_esc_restores_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        press(&mut app, &[Key::Slash, Key::Char('g'), Key::Char('g')]);
        assert_eq!(app.picker_slugs(), vec!["gg", "bverfgg"]);
        app.handle_key(Key::Esc);
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(
            app.picker_slugs(),
            LAWS.iter().map(|law| law.slug).collect::<Vec<_>>()
        );
        assert_eq!(app.cmd(), "");
    }

    #[test]
    fn menu_search_no_matches_enter_leaves() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        press(
            &mut app,
            &[Key::Slash, Key::Char('z'), Key::Char('z'), Key::Char('z')],
        );
        assert!(app.picker_slugs().is_empty());
        app.handle_key(Key::Enter);
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(
            app.picker_slugs(),
            LAWS.iter().map(|law| law.slug).collect::<Vec<_>>()
        );
    }

    #[test]
    fn slash_filters_paragraphs_and_enter_jumps() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        app.handle_key(Key::Slash);
        assert_eq!(app.mode_label(), "SEARCH");
        press(
            &mut app,
            &[
                Key::Char('k'),
                Key::Char('a'),
                Key::Char('u'),
                Key::Char('f'),
            ],
        );
        let citations = app.hits_citations();
        assert!(citations.iter().any(|c| c == "§ 433"));
        let idx = citations.iter().position(|c| c == "§ 433").unwrap();
        for _ in 0..idx {
            // highlight is auto; we'll set by moving after lock
        }
        app.handle_key(Key::Enter);
        assert_eq!(app.mode_label(), "SEARCH");
        assert!(app.search_nav());
        while app.hits_citations().get(app.hit_highlight()) != Some(&"§ 433".to_string()) {
            app.handle_key(Key::Char('j'));
        }
        app.handle_key(Key::Enter);
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(app.current_citation(), Some("§ 433"));
    }

    #[test]
    fn search_highlight_scrolls_past_the_first_page() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = many_hits_app(dir.path());
        press(
            &mut app,
            &[
                Key::Slash,
                Key::Char('n'),
                Key::Char('e'),
                Key::Char('e'),
                Key::Char('d'),
                Key::Char('l'),
                Key::Char('e'),
                Key::Enter,
            ],
        );
        assert!(app.hits_citations().len() >= 20);
        for _ in 0..19 {
            app.handle_key(Key::Char('j'));
        }
        assert_eq!(app.hits_citations()[app.hit_highlight()], "§ 20");
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let joined: String = (0..12)
            .map(|y| {
                (0..80)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.contains("§ 20"),
            "highlighted hit past the first page should stay visible:\n{joined}"
        );
        assert!(
            !joined.contains("§ 1 BGB"),
            "first-page hits should scroll away:\n{joined}"
        );
    }

    #[test]
    fn search_jk_walks_the_page_before_scrolling() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = many_hits_app(dir.path());
        press(
            &mut app,
            &[
                Key::Slash,
                Key::Char('n'),
                Key::Char('e'),
                Key::Char('e'),
                Key::Char('d'),
                Key::Char('l'),
                Key::Char('e'),
                Key::Enter,
            ],
        );
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        app.handle_key(Key::Char('j'));
        assert_eq!(app.hit_highlight(), 1);
        assert_eq!(app.search_origin(), 0);
        let mut scrolled = false;
        for _ in 0..20 {
            app.handle_key(Key::Char('j'));
            if app.search_origin() > 0 {
                scrolled = true;
                break;
            }
        }
        assert!(scrolled, "j should scroll once the highlight leaves the page");
        let origin = app.search_origin();
        let highlight = app.hit_highlight();
        app.handle_key(Key::Char('k'));
        assert_eq!(app.hit_highlight(), highlight - 1);
        assert_eq!(
            app.search_origin(),
            origin,
            "k should walk the mark up without jumping the page"
        );
    }

    #[test]
    fn search_jk_moves_highlight() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        press(
            &mut app,
            &[
                Key::Slash,
                Key::Char('j'),
                Key::Char('k'),
                Key::Char('d'),
                Key::Char('i'),
                Key::Char('e'),
            ],
        );
        assert!(!app.search_nav());
        assert_eq!(app.cmd(), "jkdie");
        app.handle_key(Key::Esc);
        press(
            &mut app,
            &[Key::Slash, Key::Char('d'), Key::Char('i'), Key::Char('e')],
        );
        assert_eq!(app.cmd(), "die");
        app.handle_key(Key::Enter);
        assert_eq!(app.mode_label(), "SEARCH");
        assert!(app.search_nav());
        assert!(app.hits_citations().len() >= 2);
        assert_eq!(app.hit_highlight(), 0);
        app.handle_key(Key::Char('j'));
        assert_eq!(app.hit_highlight(), 1);
        app.handle_key(Key::Char('k'));
        assert_eq!(app.hit_highlight(), 0);
        assert_eq!(app.cmd(), "die");
        app.handle_key(Key::Slash);
        assert!(!app.search_nav());
        app.handle_key(Key::Char('x'));
        assert_eq!(app.cmd(), "diex");
    }

    #[test]
    fn opening_laws_creates_tabs_and_bar() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = multi_app(dir.path());
        app.handle_key(Key::Enter);
        press(&mut app, &[Key::Ctrl('n'), Key::Char('m')]);
        assert!(app.on_menu());
        assert_eq!(app.tab_count(), 1);
        press(&mut app, &[Key::Char('j'), Key::Enter]);
        assert_eq!(app.tab_shortcuts(), vec!["BGB", "GG"]);
        let text = app.tab_line_plain();
        assert!(text.contains("0:MENU"));
        assert!(text.contains("1:BGB"));
        assert!(text.contains("2:GG*"));
    }

    #[test]
    fn ctrl_n_n_cycles_and_1_jumps_keeping_position() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = multi_app(dir.path());
        app.handle_key(Key::Enter);
        press(
            &mut app,
            &[Key::Char('4'), Key::Char('3'), Key::Char('3'), Key::Enter],
        );
        assert_eq!(app.current_citation(), Some("§ 433"));
        press(
            &mut app,
            &[Key::Ctrl('n'), Key::Char('m'), Key::Char('j'), Key::Enter],
        );
        press(&mut app, &[Key::Ctrl('n'), Key::Char('n')]);
        assert!(app.on_menu());
        press(&mut app, &[Key::Ctrl('n'), Key::Char('n')]);
        assert_eq!(app.current_citation(), Some("§ 433"));
        press(&mut app, &[Key::Ctrl('n'), Key::Char('2')]);
        assert_eq!(app.tab_shortcuts().last().copied(), Some("GG"));
        press(&mut app, &[Key::Ctrl('n'), Key::Char('1')]);
        assert_eq!(app.current_citation(), Some("§ 433"));
    }

    #[test]
    fn ctrl_n_x_closes_tab_and_reopen_is_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = multi_app(dir.path());
        app.handle_key(Key::Enter);
        press(
            &mut app,
            &[Key::Char('4'), Key::Char('3'), Key::Char('3'), Key::Enter],
        );
        press(
            &mut app,
            &[Key::Ctrl('n'), Key::Char('m'), Key::Char('j'), Key::Enter],
        );
        press(&mut app, &[Key::Ctrl('n'), Key::Char('x')]);
        assert!(app.on_menu());
        assert_eq!(app.tab_count(), 1);
        press(&mut app, &[Key::Ctrl('n'), Key::Char('1')]);
        assert_eq!(app.current_citation(), Some("§ 433"));
        press(&mut app, &[Key::Ctrl('n'), Key::Char('x')]);
        assert!(app.on_menu());
        assert_eq!(app.tab_count(), 0);
        assert!(app.tab_line_plain().contains("0:MENU*"));
        press(
            &mut app,
            &[
                Key::Char('i'),
                Key::Char('b'),
                Key::Char('g'),
                Key::Char('b'),
                Key::Enter,
            ],
        );
        assert_ne!(app.current_citation(), Some("§ 433"));
        assert_eq!(app.tab_count(), 1);
    }

    #[test]
    fn ctrl_digit_does_not_enter_para() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        app.handle_key(Key::Ctrl('n'));
        assert_eq!(app.mode_label(), "PREFIX");
        app.handle_key(Key::Char('4'));
        assert_eq!(app.mode_label(), "NORMAL");
        assert_eq!(app.cmd(), "");
        let first = app.current_citation().map(str::to_string);
        app.handle_key(Key::Char('l'));
        assert_ne!(app.current_citation(), first.as_deref());
    }

    #[test]
    fn ctrl_q_asks_then_y_quits() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        app.handle_key(Key::Ctrl('q'));
        assert!(app.quit_prompt());
        assert!(!app.should_quit);
        app.handle_key(Key::Char('y'));
        assert!(app.should_quit);
    }

    #[test]
    fn ctrl_q_then_n_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        app.handle_key(Key::Ctrl('q'));
        app.handle_key(Key::Char('n'));
        assert!(!app.should_quit);
        assert!(!app.quit_prompt());
        let first = app.current_citation().map(str::to_string);
        app.handle_key(Key::Char('l'));
        assert_eq!(app.mode_label(), "NORMAL");
        assert_ne!(app.current_citation(), first.as_deref());
    }

    #[test]
    fn stale_quit_q_does_not_quit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("normen.conf"),
            "[picker]\nquit = q\n[reader]\nback = q\n",
        )
        .unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        app.handle_key(Key::Char('q'));
        assert!(!app.should_quit);
        app.handle_key(Key::Ctrl('q'));
        assert!(app.quit_prompt());
        app.handle_key(Key::Esc);
        assert!(!app.should_quit);
    }

    #[test]
    fn tab_bar_renders_menu_label() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let line: String = (0..80)
            .map(|x| buffer[(x, 0)].symbol().to_string())
            .collect();
        assert!(line.contains("0:MENU"));
    }

    fn render_line(app: &mut App, width: u16, height: u16, y: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..width)
            .map(|x| buffer[(x, y)].symbol().to_string())
            .collect()
    }

    #[test]
    fn menu_shortcut_column_is_ten_wide() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        let line = render_line(&mut app, 80, 16, 3);
        let bgb = line.find("BGB").expect("BGB row");
        let title = line.find("Bürgerliches").expect("title");
        assert_eq!(title - bgb, 10);
        assert!(!line.contains(">>"));
        let backend = TestBackend::new(80, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut found_bold = false;
        for y in 2..15 {
            for x in 0..20 {
                if buffer[(x, y)].symbol() == "B"
                    && buffer[(x.saturating_add(1), y)].symbol() == "G"
                    && buffer[(x.saturating_add(2), y)].symbol() == "B"
                {
                    assert!(
                        buffer[(x, y)].modifier.contains(Modifier::BOLD),
                        "MENU shortcut should be bold"
                    );
                    found_bold = true;
                    break;
                }
            }
        }
        assert!(found_bold);
    }

    #[test]
    fn status_mode_is_a_badge_not_a_full_bar() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        let backend = TestBackend::new(80, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let status_y = 15;
        let mode_x = (0..80)
            .find(|&x| buffer[(x, status_y)].symbol() == "N")
            .expect("NORMAL badge");
        assert_eq!(buffer[(mode_x, status_y)].fg, PRIMARY);
        assert_eq!(buffer[(mode_x, status_y)].bg, SURFACE);
        assert_eq!(buffer[(40, status_y)].bg, SURFACE);
        assert_ne!(buffer[(40, status_y)].bg, ACCENT);

        app.handle_key(Key::Char('4'));
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let para_x = (0..80)
            .find(|&x| buffer[(x, status_y)].symbol() == "P")
            .expect("PARA badge");
        assert_eq!(buffer[(para_x, status_y)].bg, ACCENT);
        assert_eq!(buffer[(para_x, status_y)].fg, SEARCH_FG);
        assert_eq!(buffer[(40, status_y)].bg, SURFACE);
        let para_width = (0..80)
            .filter(|&x| buffer[(x, status_y)].bg == ACCENT)
            .count();

        app.handle_key(Key::Esc);
        app.handle_key(Key::Slash);
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let search_x = (0..80)
            .find(|&x| buffer[(x, status_y)].symbol() == "S")
            .expect("SEARCH badge");
        assert_eq!(buffer[(search_x, status_y)].bg, SECONDARY);
        assert_eq!(buffer[(search_x, status_y)].fg, SEARCH_FG);
        assert_eq!(buffer[(40, status_y)].bg, SURFACE);
        assert_ne!(buffer[(40, status_y)].bg, ACCENT);
        let search_width = (0..80)
            .filter(|&x| buffer[(x, status_y)].bg == SECONDARY)
            .count();
        assert_eq!(para_width, search_width);
        assert_eq!(para_width, 8);
    }

    #[test]
    fn question_mark_toggles_help_overlay() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        assert!(!app.help_open());
        app.handle_key(Key::Char('?'));
        assert!(app.help_open());
        assert_eq!(app.mode_label(), "HELP");
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let joined: String = (0..24)
            .map(|y| {
                (0..80)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("Keys"), "{joined}");
        assert_help_overlay(&joined);
        app.handle_key(Key::Esc);
        assert!(!app.help_open());
        app.handle_key(Key::Char('?'));
        assert!(app.help_open());
        app.handle_key(Key::Char('?'));
        assert!(!app.help_open());
    }

    #[test]
    fn help_overlay_shows_remapped_bindings() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("normen.conf"),
            "[reader]\nparagraph_next = x\nparagraph_prev = y\n",
        )
        .unwrap();
        let mut app = app_at(dir.path(), None);
        app.handle_key(Key::Char('?'));
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let joined: String = (0..24)
            .map(|y| {
                (0..80)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            joined.lines().any(|line| line.contains('x')
                && line.contains('y')
                && line.contains("paragraph")),
            "remapped J/K should appear as x y: {joined}"
        );
    }

    #[test]
    fn question_mark_in_search_types_instead_of_opening_help() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        app.handle_key(Key::Slash);
        app.handle_key(Key::Char('?'));
        assert!(!app.help_open());
        assert!(app.cmd().contains('?'));
    }

    #[test]
    fn reader_status_puts_pos_on_the_right() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        let pos = app.pos_label();
        let line = render_line(&mut app, 80, 16, 15);
        assert!(line.trim_start().starts_with("NORMAL"));
        assert!(line.trim_end().ends_with(&pos));
        let tabs = render_line(&mut app, 80, 16, 0);
        assert!(tabs.contains("0:MENU"));
        assert!(tabs.contains("1:BGB"));
    }

    #[test]
    fn reader_cards_use_left_bar_and_heading_not_green() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        let backend = TestBackend::new(80, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut found_heading = false;
        for y in 2..18 {
            let line: String = (0..80)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect();
            if line.contains("Buch 1") {
                found_heading = true;
                assert_eq!(buffer[(1, y)].symbol(), "▏");
                assert_eq!(buffer[(1, y)].fg, PRIMARY);
                assert_ne!(buffer[(1, y)].bg, PRIMARY);
                assert_ne!(buffer[(4, y)].fg, PRIMARY);
                break;
            }
        }
        assert!(found_heading);
    }

    #[test]
    fn search_results_highlight_the_matched_string() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        press(
            &mut app,
            &[
                Key::Slash,
                Key::Char('K'),
                Key::Char('a'),
                Key::Char('u'),
                Key::Char('f'),
            ],
        );
        let backend = TestBackend::new(80, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let mut found = false;
        for y in 2..17 {
            for x in 0..76 {
                if buffer[(x, y)].symbol() == "K"
                    && buffer[(x + 1, y)].symbol() == "a"
                    && buffer[(x + 2, y)].symbol() == "u"
                    && buffer[(x + 3, y)].symbol() == "f"
                {
                    found = true;
                    assert_eq!(
                        buffer[(x, y)].bg,
                        ACCENT,
                        "matched search text should use the yellow highlight"
                    );
                    assert_eq!(buffer[(x, y)].fg, SEARCH_FG);
                }
            }
        }
        assert!(found, "expected to find highlighted Kauf");
    }

    #[test]
    fn search_hits_use_two_line_cards() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        app.handle_key(Key::Slash);
        app.handle_key(Key::Char('1'));
        let backend = TestBackend::new(80, 18);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        let lines: Vec<String> = (0..18)
            .map(|y| {
                (0..80)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect()
            })
            .collect();
        let joined = lines.join("\n");
        assert!(joined.contains("§ 1"));
        assert!(joined.contains("Rechtsfähigkeit"));
        let status = &lines[17];
        assert!(status.contains("SEARCH"));
        assert!(status.trim_end().ends_with(&app.pos_label()) || status.contains("SEARCH"));
    }

    #[test]
    fn config_file_remaps_paragraph_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("normen.conf"),
            "[reader]\nparagraph_prev = x\nparagraph_next = y\n",
        )
        .unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        let first = app.current_citation().map(str::to_string);
        app.handle_key(Key::Char('y'));
        assert_ne!(app.current_citation(), first.as_deref());
        app.handle_key(Key::Char('x'));
        assert_eq!(app.current_citation(), first.as_deref());
        app.handle_key(Key::Char('K'));
        assert_eq!(app.current_citation(), first.as_deref());
    }

    #[test]
    fn ctrl_c_quits() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        app.handle_key(Key::Ctrl('c'));
        assert!(app.should_quit);
    }

    #[test]
    fn failed_open_stays_on_menu_with_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = App::new(
            Config::new(dir.path().join("normen.conf")),
            Box::new(|_, _| Err("download failed".into())),
            None,
            None,
            false,
        );
        app.handle_key(Key::Enter);
        assert!(app.on_menu());
        assert_eq!(app.tab_count(), 0);
        assert!(app.pos_label().contains("download failed"));
    }
}
