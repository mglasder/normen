use crate::catalog::{filter_from, resolve_in, LawRef};

use super::prompt::{Phase, SearchPrompt};
use super::Mode;

pub struct MenuState {
    pub mode: Mode,
    pub prompt: Option<SearchPrompt>,
    pub cmd: String,
    pub highlight: usize,
    pub core: Vec<LawRef>,
    pub filtered: Vec<LawRef>,
}

impl MenuState {
    pub fn new(core: Vec<LawRef>) -> Self {
        let filtered = core.clone();
        Self {
            mode: Mode::Normal,
            prompt: None,
            cmd: String::new(),
            highlight: 0,
            core,
            filtered,
        }
    }

    pub fn set_core(&mut self, core: Vec<LawRef>) {
        self.core = core;
        self.reset_list();
    }

    pub fn search_nav(&self) -> bool {
        self.prompt
            .as_ref()
            .is_some_and(|p| p.phase == Phase::Navigating)
    }

    pub fn search_query(&self) -> &str {
        self.prompt
            .as_ref()
            .map(SearchPrompt::filter_query)
            .unwrap_or("")
    }

    pub fn reset_list(&mut self) {
        self.cmd.clear();
        self.prompt = None;
        self.filtered = self.core.clone();
        self.highlight = 0;
        self.mode = Mode::Normal;
    }

    pub fn enter_insert(&mut self) {
        self.mode = Mode::Insert;
        self.cmd.clear();
    }

    pub fn enter_search(&mut self) {
        if self.mode == Mode::Search {
            self.resume_typing();
            return;
        }
        self.prompt = Some(SearchPrompt::enter());
        self.apply_filter();
        self.mode = Mode::Search;
    }

    pub fn resume_typing(&mut self) {
        if let Some(prompt) = &mut self.prompt {
            prompt.resume_typing();
        }
    }

    pub fn apply_filter(&mut self) {
        let query = self
            .prompt
            .as_ref()
            .map(SearchPrompt::filter_query)
            .unwrap_or("")
            .to_string();
        let keep = self.filtered.get(self.highlight).map(|law| law.slug.clone());
        self.filtered = filter_from(&query, &self.core)
            .into_iter()
            .cloned()
            .collect();
        self.highlight = keep
            .and_then(|slug| self.filtered.iter().position(|law| law.slug == slug))
            .unwrap_or(0);
        if self.filtered.is_empty() {
            self.highlight = 0;
        } else {
            self.highlight = self.highlight.min(self.filtered.len() - 1);
        }
    }

    pub fn move_highlight(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            return;
        }
        let next = self.highlight as isize + delta;
        self.highlight = next.clamp(0, self.filtered.len() as isize - 1) as usize;
    }

    pub fn highlighted(&self) -> Option<&LawRef> {
        self.filtered.get(self.highlight)
    }

    pub fn type_char(&mut self, ch: char) {
        self.cmd.push(ch);
    }

    pub fn backspace(&mut self) {
        self.cmd.pop();
    }

    pub fn type_search(&mut self, ch: char) {
        if let Some(prompt) = &mut self.prompt {
            prompt.type_char(ch);
        }
        self.apply_filter();
    }

    pub fn search_backspace(&mut self) {
        if let Some(prompt) = &mut self.prompt {
            prompt.backspace();
        }
        self.apply_filter();
    }

    pub fn lock_search(&mut self) -> bool {
        let count = self.filtered.len();
        let locked = self.prompt.as_mut().is_some_and(|p| p.lock(count));
        if !locked {
            self.reset_list();
            return false;
        }
        true
    }

    pub fn resolve_insert(&mut self) -> Option<LawRef> {
        let query = self.cmd.trim();
        if query.is_empty() {
            self.mode = Mode::Normal;
            return self.highlighted().cloned();
        }
        match resolve_in(query, &self.core).cloned() {
            Some(law_ref) => {
                self.reset_list();
                Some(law_ref)
            }
            None => {
                self.mode = Mode::Insert;
                None
            }
        }
    }
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new(crate::catalog::default_core())
    }
}

pub fn para_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '§' | ' ' | '.')
}
