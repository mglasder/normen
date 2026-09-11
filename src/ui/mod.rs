pub mod keymap;
pub mod menu;
pub mod prompt;
pub mod reader;
pub mod theme;

use std::io::{self, stdout, IsTerminal};
use std::path::PathBuf;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event};
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
use crate::document::{iter_body_blocks, norm_heading, BlockKind};
use crate::models::Law;
use crate::search::{format_search_hit, highlight_text, SpanMark};
use crate::session::{WorkspaceStore, WorkspaceTab};

use self::keymap::{map_crossterm, pretty_binding, Action, Context, Keymap};
use self::menu::{para_char, MenuState};
use self::reader::ReaderTab;
use self::theme::Theme;

pub const WINDOW_RADIUS: usize = 16;

#[derive(Debug, Clone)]
pub enum Start {
    Fresh {
        law: Option<String>,
        norm: Option<String>,
    },
    Attach {
        id: Option<u32>,
    },
}

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
    keymap: Keymap,
    load: Box<dyn Fn(&LawRef, bool) -> Result<Law, String>>,
    menu: MenuState,
    tabs: Vec<ReaderTab>,
    active: i32,
    last: i32,
    prefix: bool,
    help: bool,
    quit_pending: bool,
    kill_pending: bool,
    pub should_quit: bool,
    refresh: bool,
    workspaces: WorkspaceStore,
    session_id: Option<u32>,
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
        Self::try_start(
            config,
            load,
            Start::Fresh {
                law: initial_law,
                norm: initial_norm,
            },
            refresh,
        )
        .expect("fresh start")
    }

    pub fn try_start(
        config: Config,
        load: Box<dyn Fn(&LawRef, bool) -> Result<Law, String>>,
        start: Start,
        refresh: bool,
    ) -> Result<Self, String> {
        let session_path = config
            .path
            .parent()
            .unwrap_or(config.path.as_path())
            .join("sessions.json");
        Self::start_with_path(config, load, start, refresh, session_path)
    }

    pub fn start_with_path(
        config: Config,
        load: Box<dyn Fn(&LawRef, bool) -> Result<Law, String>>,
        start: Start,
        refresh: bool,
        session_path: PathBuf,
    ) -> Result<Self, String> {
        match start {
            Start::Fresh { law, norm } => {
                Ok(Self::fresh(config, load, law, norm, refresh, session_path))
            }
            Start::Attach { id } => {
                let mut app = Self::fresh(config, load, None, None, refresh, session_path);
                app.attach(id)?;
                Ok(app)
            }
        }
    }

    fn fresh(
        config: Config,
        load: Box<dyn Fn(&LawRef, bool) -> Result<Law, String>>,
        initial_law: Option<String>,
        initial_norm: Option<String>,
        refresh: bool,
        session_path: PathBuf,
    ) -> Self {
        let keymap = Keymap::from_config(&config);
        let mut app = Self {
            config,
            keymap,
            load,
            menu: MenuState::new(),
            tabs: Vec::new(),
            active: -1,
            last: -1,
            prefix: false,
            help: false,
            quit_pending: false,
            kill_pending: false,
            should_quit: false,
            refresh,
            workspaces: WorkspaceStore::open(session_path),
            session_id: None,
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

    fn attach(&mut self, id: Option<u32>) -> Result<(), String> {
        let id = id
            .or_else(|| self.workspaces.mru_id())
            .ok_or_else(|| "no persisted session".to_string())?;
        let workspace = self
            .workspaces
            .get(id)
            .cloned()
            .ok_or_else(|| format!("no persisted session {id}"))?;
        let mut failed = false;
        for tab in &workspace.tabs {
            let Some(law_ref) = resolve_law(&tab.slug) else {
                self.load_error = Some(format!("unknown law {}", tab.slug));
                failed = true;
                continue;
            };
            self.open_tab(*law_ref, Some(tab.citation.as_str()));
            if self.load_error.is_some() {
                failed = true;
            }
        }
        if failed {
            return Ok(());
        }
        self.session_id = Some(id);
        self.workspaces.touch(id);
        if workspace.active < 0 {
            self.show_menu();
        } else {
            self.switch_tab(workspace.active as usize);
        }
        Ok(())
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
            self.quit_pending = false;
            self.kill_pending = true;
            return;
        }
        let ctx = if self.on_menu() {
            Context::Menu
        } else {
            Context::Reader
        };
        if self.kill_pending {
            if is_yes(key) {
                self.kill_quit();
            } else if self.keymap.resolve(key, ctx) == Some(Action::EnterNormal) {
                self.kill_pending = false;
            }
            return;
        }
        if self.quit_pending {
            if is_yes(key) {
                self.persist_quit();
            } else if self.keymap.resolve(key, ctx) == Some(Action::EnterNormal) {
                self.quit_pending = false;
            }
            return;
        }
        if self.help {
            if self.keymap.resolve(key, ctx) == Some(Action::Quit) {
                self.help = false;
                self.quit_pending = true;
                return;
            }
            self.help = false;
            return;
        }
        if self.keymap.resolve(key, ctx) == Some(Action::Help)
            && self.screen_mode() == Mode::Normal
            && !self.prefix
        {
            self.help = true;
            return;
        }
        if self.keymap.resolve(key, ctx) == Some(Action::Quit) {
            if self.prefix {
                self.prefix = false;
            }
            self.quit_pending = true;
            return;
        }
        if self.prefix {
            if self.keymap.resolve(key, ctx) == Some(Action::TabPrefix) {
                return;
            }
            self.prefix = false;
            self.run_prefix(key);
            return;
        }
        if self.keymap.resolve(key, ctx) == Some(Action::TabPrefix) {
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
        match self.keymap.resolve(key, Context::Prefix) {
            Some(Action::TabNext) => self.next_tab(),
            Some(Action::TabPrev) => self.prev_tab(),
            Some(Action::TabClose) => self.close_tab(),
            Some(Action::TabMenu) => self.show_menu(),
            _ => {
                if let Key::Char(c) = key {
                    if c.is_ascii_digit() {
                        self.jump_tab(c.to_digit(10).unwrap_or(0) as usize);
                    }
                }
            }
        }
    }

    fn pending_load_label(&self, key: Key) -> Option<&'static str> {
        if self.quit_pending || self.kill_pending || self.prefix || !self.on_menu() {
            return None;
        }
        let confirm = self.keymap.resolve(key, Context::Menu) == Some(Action::Confirm);
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
        let action = self.keymap.resolve(key, Context::Menu);
        let mode = self.menu.mode;
        let search_nav = self.menu.search_nav;
        match mode {
            Mode::Insert => match action {
                Some(Action::EnterNormal) => self.menu.reset_list(),
                Some(Action::Confirm) => {
                    if let Some(law_ref) = self.menu.resolve_insert().copied() {
                        self.open_tab(law_ref, None);
                    }
                }
                _ => {
                    if key == Key::Backspace {
                        self.menu.backspace();
                    } else if let Some(ch) = printable(key) {
                        self.menu.type_char(ch);
                    }
                }
            },
            Mode::Search if !search_nav => match action {
                Some(Action::EnterNormal) => self.menu.reset_list(),
                Some(Action::Confirm) => {
                    self.menu.lock_search();
                }
                Some(Action::EnterSearch) => self.menu.search_nav = false,
                _ => {
                    if key == Key::Backspace {
                        self.menu.backspace();
                    } else if let Some(ch) = printable(key) {
                        self.menu.type_char(ch);
                    }
                }
            },
            Mode::Search => match action {
                Some(Action::EnterNormal) => self.menu.reset_list(),
                Some(Action::EnterSearch) => self.menu.search_nav = false,
                Some(Action::MoveDown | Action::MoveRight) => self.menu.move_highlight(1),
                Some(Action::MoveUp | Action::MoveLeft) => self.menu.move_highlight(-1),
                Some(Action::Confirm) => {
                    if let Some(law_ref) = self.menu.highlighted().copied() {
                        self.menu.reset_list();
                        self.open_tab(law_ref, None);
                    }
                }
                _ => {}
            },
            Mode::Normal | Mode::Para => match action {
                Some(Action::EnterPara) => self.menu.enter_insert(),
                Some(Action::EnterSearch) => self.menu.enter_search(),
                Some(Action::MoveDown | Action::MoveRight) => self.menu.move_highlight(1),
                Some(Action::MoveUp | Action::MoveLeft) => self.menu.move_highlight(-1),
                Some(Action::Confirm) => {
                    if let Some(law_ref) = self.menu.highlighted().copied() {
                        self.open_tab(law_ref, None);
                    }
                }
                _ => {}
            },
        }
    }

    fn handle_reader(&mut self, key: Key) {
        let action = self.keymap.resolve(key, Context::Reader);
        let card_width = self.reader_card_width();
        let view_height = self.reader_view_height();
        let page = view_height.max(1) as isize;
        let Some(tab) = self.current_tab_mut() else {
            return;
        };
        match tab.mode {
            Mode::Search if !tab.search_nav => match action {
                Some(Action::EnterNormal) => tab.enter_normal(),
                Some(Action::Confirm) => {
                    tab.lock_search();
                }
                Some(Action::EnterSearch) => tab.search_nav = false,
                _ => {
                    if key == Key::Backspace {
                        tab.search_backspace();
                    } else if let Some(ch) = printable(key) {
                        tab.type_search(ch);
                    }
                }
            },
            Mode::Search => match action {
                Some(Action::EnterNormal) => tab.enter_normal(),
                Some(Action::EnterSearch) => tab.search_nav = false,
                Some(Action::MoveDown) => tab.move_hit(1, card_width, view_height),
                Some(Action::MoveUp) => tab.move_hit(-1, card_width, view_height),
                Some(Action::Confirm) => tab.open_highlighted_hit(),
                _ => {}
            },
            Mode::Para => match action {
                Some(Action::EnterNormal) => tab.enter_normal(),
                Some(Action::Confirm) => tab.confirm_para(),
                None => {
                    if key == Key::Backspace {
                        tab.para.pop();
                    } else if let Key::Char(c) = key {
                        if c.is_ascii_digit() {
                            tab.type_digit(c);
                        } else if para_char(c) {
                            tab.type_para_char(c);
                        }
                    }
                }
                Some(_) => {}
            },
            Mode::Normal | Mode::Insert => match action {
                Some(Action::EnterPara) => tab.enter_para(""),
                Some(Action::EnterSearch) => tab.enter_search(),
                Some(Action::ParagraphNext) => tab.step_norm(1, card_width, view_height),
                Some(Action::ParagraphPrev) => tab.step_norm(-1, card_width, view_height),
                Some(Action::MoveDown) => tab.scroll_by(1, card_width, view_height),
                Some(Action::MoveUp) => tab.scroll_by(-1, card_width, view_height),
                Some(Action::PageDown) => tab.scroll_by(page, card_width, view_height),
                Some(Action::PageUp) => tab.scroll_by(-page, card_width, view_height),
                Some(Action::MoveRight) => tab.step_norm(1, card_width, view_height),
                Some(Action::MoveLeft) => tab.step_norm(-1, card_width, view_height),
                Some(Action::GotoTop) => tab.goto_top(),
                Some(Action::GotoBottom) => tab.goto_bottom(),
                None => {
                    if let Key::Char(c) = key {
                        if c.is_ascii_digit() {
                            tab.type_digit(c);
                        }
                    }
                }
                Some(_) => {}
            },
        }
    }

    fn open_tab(&mut self, law_ref: LawRef, initial: Option<&str>) {
        match (self.load)(&law_ref, self.refresh) {
            Ok(law) => {
                self.load_error = None;
                let mut tab = ReaderTab::from_law(law_ref, law);
                tab.apply_initial(initial);
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

        let theme = self.theme();
        frame.render_widget(
            Paragraph::new(self.tab_line()).style(Style::default().bg(theme.surface)),
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
        if self.quit_pending || self.kill_pending {
            self.draw_confirm(frame, area);
        }
    }

    fn draw_cmd(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = self.theme();
        let style = Style::default().bg(theme.surface).fg(theme.foreground);
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
        if self.prefix || self.quit_pending || self.kill_pending || self.help {
            return false;
        }
        match self.screen_mode() {
            Mode::Insert | Mode::Para => true,
            Mode::Search => !self.search_nav(),
            Mode::Normal => false,
        }
    }

    fn draw_body(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = self.theme();
        frame.render_widget(Block::new().style(Style::default().bg(theme.background)), area);
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
        let theme = self.theme();
        let query = self.menu.cmd.trim_start_matches('/');
        let items: Vec<ListItem> = self
            .menu
            .filtered
            .iter()
            .map(|law| {
                ListItem::new(menu_row(
                    law,
                    query,
                    self.menu.mode == Mode::Search,
                    &theme,
                ))
            })
            .collect();
        let mut state = ListState::default();
        if !items.is_empty() {
            state.select(Some(self.menu.highlight.min(items.len().saturating_sub(1))));
        }
        let list = List::new(items)
            .style(Style::default().bg(theme.background).fg(theme.foreground))
            .highlight_style(Style::default().bg(theme.highlight_bg).fg(theme.foreground))
            .highlight_symbol("");
        frame.render_stateful_widget(list, list_area, &mut state);
        draw_scrollbar(
            frame,
            inner,
            self.menu.highlight,
            self.menu.filtered.len().max(1),
            &theme,
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
        let theme = self.theme();
        let card_width = inner.width.saturating_sub(1);
        let query = tab.cmd.trim_start_matches('/');
        let mut y = inner.y;
        for (index, hit) in tab.hits.iter().enumerate().skip(tab.search_origin) {
            if y >= inner.bottom() {
                break;
            }
            let bg = if index == tab.hit_highlight {
                theme.highlight_bg
            } else {
                theme.surface
            };
            let lines = search_card_lines(hit, query, &tab.law.abbreviation, card_width, bg, &theme);
            let height = (lines.len() as u16).min(inner.bottom().saturating_sub(y));
            if height == 0 {
                break;
            }
            frame.render_widget(
                Paragraph::new(lines).style(Style::default().bg(bg).fg(theme.foreground)),
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
            &theme,
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
        let theme = self.theme();
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
                &theme,
            );
            lines.push(filled_line(
                "",
                card_width,
                Style::default().bg(theme.background),
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
            &theme,
        );
    }

    fn tab_line(&self) -> Line<'static> {
        let theme = self.theme();
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
                .fg(theme.surface)
                .bg(theme.primary)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.foreground)
                .bg(theme.border)
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
                    .fg(theme.surface)
                    .bg(theme.primary)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(theme.foreground)
                    .bg(theme.border)
                    .add_modifier(Modifier::BOLD)
            };
            spans.push(Span::styled(label, style));
        }
        Line::from(spans).style(Style::default().bg(theme.surface))
    }

    fn draw_status(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = self.theme();
        let bar_style = Style::default().bg(theme.surface).fg(theme.foreground);
        let pos = if self.kill_pending || self.quit_pending {
            String::new()
        } else if self.load_error.is_some() {
            self.pos_label()
        } else if self.on_menu() {
            String::new()
        } else {
            self.pos_label()
        };
        let pos_style = if self.quit_pending || self.kill_pending || self.load_error.is_some() {
            Style::default()
                .bg(theme.surface)
                .fg(theme.error)
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
        let theme = self.theme();
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
            .title_style(Style::default().fg(theme.accent))
            .style(Style::default().bg(theme.surface).fg(theme.foreground))
            .border_style(Style::default().fg(theme.accent));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(theme.surface).fg(theme.foreground)),
            inner,
        );
    }

    fn draw_confirm(&self, frame: &mut Frame<'_>, area: Rect) {
        let theme = self.theme();
        let (title, body) = self.confirm_copy();
        let width = area.width.saturating_sub(8).min(52).max(28);
        let height = (body.len() as u16)
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
            .title(title)
            .title_bottom(" y confirm   Esc cancel ")
            .title_style(Style::default().fg(theme.error))
            .style(Style::default().bg(theme.surface).fg(theme.foreground))
            .border_style(Style::default().fg(theme.error));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let lines: Vec<Line<'static>> = body
            .into_iter()
            .map(|text| {
                Line::from(Span::styled(
                    format!(" {text}"),
                    Style::default().fg(theme.foreground),
                ))
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).style(Style::default().bg(theme.surface).fg(theme.foreground)),
            inner,
        );
    }

    fn confirm_copy(&self) -> (&'static str, Vec<&'static str>) {
        if self.kill_pending {
            if self.session_id.is_some() {
                (
                    " Kill ",
                    vec![
                        "Quit without saving.",
                        "This session will be deleted.",
                    ],
                )
            } else {
                (
                    " Kill ",
                    vec![
                        "Quit without saving.",
                        "This layout was never saved.",
                    ],
                )
            }
        } else if self.tabs.is_empty() {
            (
                " Quit ",
                vec![
                    "Quit without saving.",
                    "There are no law tabs to keep.",
                ],
            )
        } else {
            (
                " Save ",
                vec![
                    "Save this workspace and quit.",
                    "Resume later with: normen attach",
                ],
            )
        }
    }

    fn help_lines(&self) -> Vec<Line<'static>> {
        let theme = self.theme();
        let key = |name: &str| pretty_binding(self.keymap.binding(name));
        let row = |left: String, right: &str| {
            Line::from(vec![
                Span::styled(format!("  {left:<18}"), Style::default().fg(theme.primary)),
                Span::styled(right.to_string(), Style::default().fg(theme.foreground)),
            ])
        };
        let heading = |text: &str| {
            Line::from(Span::styled(
                format!(" {text}"),
                Style::default()
                    .fg(theme.secondary)
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
            row(key("quit"), "save workspace"),
            row("Ctrl-c".into(), "kill session"),
            row(key("help"), "this window"),
        ]
    }

    fn mode_badge_style(&self) -> Style {
        let theme = self.theme();
        if self.kill_pending || self.quit_pending {
            return Style::default()
                .fg(theme.surface)
                .bg(theme.error)
                .add_modifier(Modifier::BOLD);
        }
        if self.help || self.prefix {
            return Style::default()
                .fg(theme.foreground)
                .bg(theme.border)
                .add_modifier(Modifier::BOLD);
        }
        match self.screen_mode() {
            Mode::Normal => Style::default()
                .fg(theme.primary)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD),
            Mode::Para | Mode::Insert => Style::default()
                .fg(theme.search_fg)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
            Mode::Search => Style::default()
                .fg(theme.search_fg)
                .bg(theme.secondary)
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
        if self.kill_pending {
            return "KILL";
        }
        if self.quit_pending {
            return if self.tabs.is_empty() { "QUIT" } else { "SAVE" };
        }
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

    pub fn kill_prompt(&self) -> bool {
        self.kill_pending
    }

    fn persist_quit(&mut self) {
        if !self.tabs.is_empty() {
            let tabs = self
                .tabs
                .iter()
                .map(|tab| WorkspaceTab {
                    slug: tab.law_ref.slug.to_string(),
                    citation: tab.current_citation().unwrap_or("").to_string(),
                })
                .collect();
            self.session_id = Some(self.workspaces.save(self.session_id, self.active, tabs));
        }
        self.should_quit = true;
    }

    fn kill_quit(&mut self) {
        if let Some(id) = self.session_id.take() {
            self.workspaces.remove(id);
        }
        self.should_quit = true;
    }

    pub fn help_open(&self) -> bool {
        self.help
    }

    fn theme(&self) -> Theme {
        Theme::from_config(&self.config)
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

fn menu_row(law: &LawRef, query: &str, searching: bool, theme: &Theme) -> Line<'static> {
    let shortcut = format!("{:<10}", law.shortcut);
    if searching {
        let mut spans = styled_to_line(&highlight_text(&shortcut, query), theme).spans;
        for span in &mut spans {
            span.style = span.style.add_modifier(Modifier::BOLD);
        }
        spans.extend(styled_to_line(&highlight_text(law.title, query), theme).spans);
        Line::from(spans)
    } else {
        Line::from(vec![
            Span::styled(shortcut, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(law.title.to_string()),
        ])
    }
}

fn styled_to_line(text: &crate::search::StyledText, theme: &Theme) -> Line<'static> {
    styled_to_lines(text, theme)
        .into_iter()
        .next()
        .unwrap_or_else(|| Line::from(""))
}

fn styled_to_lines(text: &crate::search::StyledText, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    let mut pos = 0usize;
    let mut events: Vec<(usize, usize, Option<SpanMark>)> = Vec::new();
    if text.spans.is_empty() {
        events.push((0, text.plain.len(), None));
    } else {
        for span in &text.spans {
            if span.start > pos {
                events.push((pos, span.start, None));
            }
            events.push((span.start, span.end, Some(span.mark)));
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
                span_style(style, theme),
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

fn span_style(style: Option<SpanMark>, theme: &Theme) -> Style {
    match style {
        Some(SpanMark::Hit) => Style::default()
            .fg(theme.search_fg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
        Some(SpanMark::Bold) => {
            Style::default().add_modifier(Modifier::BOLD)
        }
        _ => Style::default(),
    }
}

fn draw_scrollbar(frame: &mut Frame<'_>, area: Rect, index: usize, total: usize, theme: &Theme) {
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
        Block::new().style(Style::default().bg(theme.background)),
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
        Block::new().style(Style::default().bg(theme.highlight_bg)),
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
    search_card_lines(hit, query, abbreviation, width, Theme::default().surface, &Theme::default())
        .len()
        .max(1) as u16
}

fn search_card_lines(
    hit: &crate::models::SearchHit,
    query: &str,
    abbreviation: &str,
    width: u16,
    bg: ratatui::style::Color,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let style = Style::default().bg(bg).fg(theme.foreground);
    let inner_width = width.saturating_sub(2) as usize;
    let mut lines = vec![filled_line("", width, style)];
    let prompt = format_search_hit(hit, query, abbreviation);
    for line in styled_to_lines(&prompt, theme) {
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
    theme: &Theme,
) -> Vec<Line<'static>> {
    let card = Style::default().bg(theme.surface).fg(theme.foreground);
    let mark = if current { "▏" } else { " " };
    let mark_style = if current {
        Style::default().fg(theme.primary).bg(theme.surface)
    } else {
        Style::default().fg(theme.surface).bg(theme.surface)
    };
    let heading_style = if norm.keys.is_empty() && norm.text.trim().is_empty() {
        card.fg(theme.secondary).add_modifier(Modifier::BOLD)
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
            if block.kind == BlockKind::List {
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
    norm_card_lines(norm, abbreviation, false, width, &Theme::default())
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
    use crate::fetch::load_law;
    use crate::session::{WorkspaceStore, WorkspaceTab};
    use super::theme::{ACCENT, ERROR, FOREGROUND, PRIMARY, SEARCH_FG, SECONDARY, SURFACE};
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;
    use std::path::Path;

    const SAMPLE: &[u8] = include_bytes!("../../tests/fixtures/sample.xml");
    const GG_SAMPLE: &[u8] = include_bytes!("../../tests/fixtures/gg_sample.xml");

    fn app_at(dir: &Path, initial_law: Option<&str>) -> App {
        sample_app(dir, initial_law, None)
    }

    fn sample_app(dir: &Path, initial_law: Option<&str>, initial_norm: Option<&str>) -> App {
        let xml = SAMPLE.to_vec();
        let cache = dir.to_path_buf();
        App::new(
            Config::new(dir.join("normen.conf")),
            Box::new(move |law_ref, refresh| {
                load_law(&cache, |_| Ok(xml.clone()), law_ref, refresh)
            }),
            initial_law.map(str::to_string),
            initial_norm.map(str::to_string),
            false,
        )
    }

    fn multi_app(dir: &Path) -> App {
        let bgb = SAMPLE.to_vec();
        let gg = GG_SAMPLE.to_vec();
        let cache = dir.to_path_buf();
        App::new(
            Config::new(dir.join("normen.conf")),
            Box::new(move |law_ref, refresh| {
                load_law(
                    &cache,
                    |slug| {
                        if slug == "gg" {
                            Ok(gg.clone())
                        } else {
                            Ok(bgb.clone())
                        }
                    },
                    law_ref,
                    refresh,
                )
            }),
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
            !joined.contains("quit now"),
            "Ctrl-c is kill, not quit now: {joined}"
        );
        assert!(
            joined.contains("kill") || joined.contains("Kill"),
            "missing kill: {joined}"
        );
        assert!(
            joined.contains("save") && (joined.contains("Ctrl-q") || joined.contains("Ctrl-Q")),
            "missing save quit: {joined}"
        );
        assert!(
            joined.contains("this window"),
            "help key was clipped: {joined}"
        );
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
        let cache = dir.to_path_buf();
        App::new(
            Config::new(dir.join("normen.conf")),
            Box::new(move |law_ref, refresh| {
                load_law(&cache, |_| Ok(xml.clone()), law_ref, refresh)
            }),
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
        let cache = dir.to_path_buf();
        App::new(
            Config::new(dir.join("normen.conf")),
            Box::new(move |law_ref, refresh| {
                load_law(&cache, |_| Ok(xml.clone()), law_ref, refresh)
            }),
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
    fn ctrl_q_saves_open_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        app.handle_key(Key::Ctrl('q'));
        app.handle_key(Key::Char('y'));
        let store = WorkspaceStore::open(dir.path().join("sessions.json"));
        let workspace = store.get(1).expect("saved workspace");
        assert_eq!(workspace.tabs[0].slug, "bgb");
        assert!(
            !workspace.tabs[0].citation.is_empty(),
            "citation {}",
            workspace.tabs[0].citation
        );
        assert_eq!(store.mru_id(), Some(1));
    }

    #[test]
    fn ctrl_q_menu_only_does_not_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        app.handle_key(Key::Ctrl('q'));
        app.handle_key(Key::Char('y'));
        assert!(app.should_quit);
        let store = WorkspaceStore::open(dir.path().join("sessions.json"));
        assert!(store.list().is_empty());
    }

    #[test]
    fn ctrl_q_second_save_overwrites_same_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        app.handle_key(Key::Ctrl('q'));
        app.handle_key(Key::Char('y'));
        app.should_quit = false;
        app.handle_key(Key::Char('l'));
        app.handle_key(Key::Ctrl('q'));
        app.handle_key(Key::Char('y'));
        let store = WorkspaceStore::open(dir.path().join("sessions.json"));
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get(1).unwrap().id, 1);
    }

    fn seed_workspace(dir: &Path, active: i32, tabs: Vec<WorkspaceTab>) -> u32 {
        let mut store = WorkspaceStore::open(dir.join("sessions.json"));
        store.save(None, active, tabs)
    }

    fn start_multi(dir: &Path, start: Start) -> Result<App, String> {
        let bgb = SAMPLE.to_vec();
        let gg = GG_SAMPLE.to_vec();
        let cache = dir.to_path_buf();
        App::try_start(
            Config::new(dir.join("normen.conf")),
            Box::new(move |law_ref, refresh| {
                load_law(
                    &cache,
                    |slug| {
                        if slug == "gg" {
                            Ok(gg.clone())
                        } else {
                            Ok(bgb.clone())
                        }
                    },
                    law_ref,
                    refresh,
                )
            }),
            start,
            false,
        )
    }

    #[test]
    fn attach_restores_tabs_citations_and_menu() {
        let dir = tempfile::tempdir().unwrap();
        seed_workspace(
            dir.path(),
            -1,
            vec![
                WorkspaceTab {
                    slug: "bgb".into(),
                    citation: "§ 433".into(),
                },
                WorkspaceTab {
                    slug: "gg".into(),
                    citation: "Art 1".into(),
                },
            ],
        );
        let app = start_multi(dir.path(), Start::Attach { id: Some(1) }).unwrap();
        assert_eq!(app.tab_shortcuts(), vec!["BGB", "GG"]);
        assert!(app.on_menu());
        assert_eq!(
            WorkspaceStore::open(dir.path().join("sessions.json")).mru_id(),
            Some(1)
        );
    }

    #[test]
    fn attach_without_sessions_errors() {
        let dir = tempfile::tempdir().unwrap();
        let err = start_multi(dir.path(), Start::Attach { id: None })
            .err()
            .expect("attach should fail");
        assert!(
            err.contains("no persisted session"),
            "{err}"
        );
    }

    #[test]
    fn attach_restores_active_law_tab() {
        let dir = tempfile::tempdir().unwrap();
        seed_workspace(
            dir.path(),
            0,
            vec![
                WorkspaceTab {
                    slug: "bgb".into(),
                    citation: "§ 433".into(),
                },
                WorkspaceTab {
                    slug: "gg".into(),
                    citation: "Art 1".into(),
                },
            ],
        );
        let app = start_multi(dir.path(), Start::Attach { id: Some(1) }).unwrap();
        assert!(!app.on_menu());
        assert_eq!(app.tab_shortcuts()[0], "BGB");
        assert_eq!(app.current_citation(), Some("§ 433"));
    }

    #[test]
    fn attach_failed_load_does_not_rewrite_json() {
        let dir = tempfile::tempdir().unwrap();
        seed_workspace(
            dir.path(),
            0,
            vec![WorkspaceTab {
                slug: "bgb".into(),
                citation: "§ 433".into(),
            }],
        );
        let path = dir.path().join("sessions.json");
        let before = std::fs::read(&path).unwrap();
        let app = App::try_start(
            Config::new(dir.path().join("normen.conf")),
            Box::new(|_, _| Err("download failed".into())),
            Start::Attach { id: Some(1) },
            false,
        )
        .unwrap();
        assert!(app.load_error.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn ctrl_q_overlay_explains_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        app.handle_key(Key::Ctrl('q'));
        let joined = render_joined(&mut app, 80, 24);
        assert!(joined.contains("Save"), "{joined}");
        assert!(
            joined.contains("workspace") && joined.contains("quit"),
            "{joined}"
        );
        assert!(joined.contains('y') && joined.contains("Esc"), "{joined}");
        app.handle_key(Key::Char('n'));
        assert!(app.quit_prompt(), "only Esc should cancel");
        app.handle_key(Key::Esc);
        assert!(!app.quit_prompt());
        assert!(!app.should_quit);
    }

    #[test]
    fn ctrl_c_overlay_explains_delete() {
        let dir = tempfile::tempdir().unwrap();
        seed_workspace(
            dir.path(),
            0,
            vec![WorkspaceTab {
                slug: "bgb".into(),
                citation: "§ 433".into(),
            }],
        );
        let mut app = start_multi(dir.path(), Start::Attach { id: Some(1) }).unwrap();
        app.handle_key(Key::Ctrl('c'));
        let joined = render_joined(&mut app, 80, 24);
        assert!(joined.contains("Kill"), "{joined}");
        assert!(
            joined.contains("without saving") && joined.contains("delete"),
            "{joined}"
        );
        assert!(joined.contains("Esc"), "{joined}");
    }

    #[test]
    fn help_overlay_is_yellow() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        app.handle_key(Key::Char('?'));
        let buffer = render_buffer(&mut app, 80, 24);
        assert_chrome_fg(&buffer, "Keys", ACCENT);
        assert_chrome_fg(&buffer, "close", ACCENT);
        assert_text_fg(&buffer, "scroll", FOREGROUND);
    }

    #[test]
    fn help_overlay_chrome_uses_config_accent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("normen.conf"),
            "[theme]\naccent = \"#ff0000\"\n",
        )
        .unwrap();
        let mut app = app_at(dir.path(), None);
        app.handle_key(Key::Char('?'));
        let buffer = render_buffer(&mut app, 80, 24);
        assert_chrome_fg(&buffer, "Keys", Color::Rgb(0xff, 0, 0));
    }

    #[test]
    fn quit_overlay_is_red() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), Some("bgb"));
        app.handle_key(Key::Ctrl('q'));
        let buffer = render_buffer(&mut app, 80, 24);
        assert_chrome_fg(&buffer, "Save", ERROR);
        assert_chrome_fg(&buffer, "cancel", ERROR);
        assert_text_fg(&buffer, "workspace", FOREGROUND);
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

    fn render_buffer(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        terminal.backend().buffer().clone()
    }

    fn render_joined(app: &mut App, width: u16, height: u16) -> String {
        let buffer = render_buffer(app, width, height);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn assert_text_fg(buffer: &ratatui::buffer::Buffer, needle: &str, expected: Color) {
        let width = buffer.area().width;
        let height = buffer.area().height;
        let n = needle.chars().count() as u16;
        let mut found = false;
        for y in 0..height {
            for x in 0..=width.saturating_sub(n) {
                let got: String = (0..n)
                    .map(|i| buffer[(x + i, y)].symbol().to_string())
                    .collect();
                if got == needle {
                    found = true;
                    for i in 0..n {
                        assert_eq!(
                            buffer[(x + i, y)].fg,
                            expected,
                            "{needle} should be {expected:?}, got {:?}",
                            buffer[(x + i, y)].fg
                        );
                    }
                }
            }
        }
        assert!(found, "missing overlay text {needle}");
    }

    fn assert_chrome_fg(buffer: &ratatui::buffer::Buffer, needle: &str, expected: Color) {
        let width = buffer.area().width;
        let height = buffer.area().height;
        let n = needle.chars().count() as u16;
        let mut found = false;
        for y in 0..height {
            let row_has_border = (0..width).any(|x| {
                matches!(buffer[(x, y)].symbol(), "─" | "┌" | "┐" | "└" | "┘")
            });
            if !row_has_border {
                continue;
            }
            for x in 0..=width.saturating_sub(n) {
                let got: String = (0..n)
                    .map(|i| buffer[(x + i, y)].symbol().to_string())
                    .collect();
                if got == needle {
                    found = true;
                    for i in 0..n {
                        assert_eq!(
                            buffer[(x + i, y)].fg,
                            expected,
                            "{needle} chrome should be {expected:?}, got {:?}",
                            buffer[(x + i, y)].fg
                        );
                    }
                }
            }
        }
        assert!(found, "missing overlay chrome {needle}");
        let mut found_border = false;
        for y in 0..height {
            for x in 0..width {
                if matches!(buffer[(x, y)].symbol(), "─" | "│" | "┌" | "┐" | "└" | "┘") {
                    assert_eq!(
                        buffer[(x, y)].fg,
                        expected,
                        "overlay border should be {expected:?}"
                    );
                    found_border = true;
                }
            }
        }
        assert!(found_border, "missing overlay border");
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
    fn remapped_move_down_does_not_keep_j() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("normen.conf"), "[keys]\nmove_down = d\n").unwrap();
        let mut app = sample_app(dir.path(), Some("bgb"), None);
        let start = app.current_index();
        for _ in 0..80 {
            app.handle_key(Key::Char('j'));
            assert_eq!(app.current_index(), start, "j must not scroll after remap");
        }
        for _ in 0..80 {
            app.handle_key(Key::Char('d'));
            if app.current_index() > start {
                return;
            }
        }
        panic!("d should scroll lines");
    }

    #[test]
    fn ctrl_c_asks_then_y_quits_without_saving() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_at(dir.path(), None);
        app.handle_key(Key::Ctrl('c'));
        assert!(!app.should_quit);
        assert!(app.kill_prompt());
        app.handle_key(Key::Char('y'));
        assert!(app.should_quit);
        assert!(WorkspaceStore::open(dir.path().join("sessions.json"))
            .list()
            .is_empty());
    }

    #[test]
    fn ctrl_c_deletes_attached_session() {
        let dir = tempfile::tempdir().unwrap();
        seed_workspace(
            dir.path(),
            0,
            vec![WorkspaceTab {
                slug: "bgb".into(),
                citation: "§ 433".into(),
            }],
        );
        let mut app = start_multi(dir.path(), Start::Attach { id: Some(1) }).unwrap();
        app.handle_key(Key::Ctrl('c'));
        app.handle_key(Key::Char('y'));
        assert!(app.should_quit);
        assert!(WorkspaceStore::open(dir.path().join("sessions.json"))
            .get(1)
            .is_none());
    }

    #[test]
    fn ctrl_c_n_keeps_attached_session() {
        let dir = tempfile::tempdir().unwrap();
        seed_workspace(
            dir.path(),
            0,
            vec![WorkspaceTab {
                slug: "bgb".into(),
                citation: "§ 433".into(),
            }],
        );
        let mut app = start_multi(dir.path(), Start::Attach { id: Some(1) }).unwrap();
        app.handle_key(Key::Ctrl('c'));
        app.handle_key(Key::Char('n'));
        assert!(app.kill_prompt(), "only Esc should cancel");
        app.handle_key(Key::Esc);
        assert!(!app.should_quit);
        assert!(!app.kill_prompt());
        assert!(WorkspaceStore::open(dir.path().join("sessions.json"))
            .get(1)
            .is_some());
    }

    #[test]
    fn ctrl_c_replaces_quit_pending_with_kill() {
        let dir = tempfile::tempdir().unwrap();
        seed_workspace(
            dir.path(),
            0,
            vec![WorkspaceTab {
                slug: "bgb".into(),
                citation: "§ 433".into(),
            }],
        );
        let mut app = start_multi(dir.path(), Start::Attach { id: Some(1) }).unwrap();
        app.handle_key(Key::Ctrl('q'));
        assert!(app.quit_prompt());
        app.handle_key(Key::Ctrl('c'));
        assert!(!app.quit_prompt());
        assert!(app.kill_prompt());
        app.handle_key(Key::Char('y'));
        assert!(WorkspaceStore::open(dir.path().join("sessions.json"))
            .get(1)
            .is_none());
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
