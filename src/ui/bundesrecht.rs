use crate::bundesrecht::filter_name;
use crate::catalog::LawRef;

use super::prompt::{Phase, SearchPrompt, search_line};
use super::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundFocus {
    Catalog,
    Marks,
}

pub struct BundesrechtOverlay {
    pub mode: Mode,
    pub prompt: Option<SearchPrompt>,
    pub highlight: usize,
    pub focus: BundFocus,
    pub mark_highlight: usize,
    pub visible: Vec<LawRef>,
    pub marks: Vec<String>,
}

impl BundesrechtOverlay {
    pub fn new(index: &[LawRef]) -> Self {
        Self {
            mode: Mode::Normal,
            prompt: None,
            highlight: 0,
            focus: BundFocus::Catalog,
            mark_highlight: 0,
            visible: index.to_vec(),
            marks: Vec::new(),
        }
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

    pub fn search_display(&self) -> String {
        search_line(self.mode == Mode::Search, self.search_query())
    }

    pub fn highlighted(&self) -> Option<&LawRef> {
        self.visible.get(self.highlight)
    }

    pub fn in_marks(&self) -> bool {
        self.focus == BundFocus::Marks && !self.marks.is_empty()
    }

    pub fn show_marks_panel(&self) -> bool {
        self.focus == BundFocus::Marks || !self.marks.is_empty()
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            BundFocus::Catalog => BundFocus::Marks,
            BundFocus::Marks => BundFocus::Catalog,
        };
        if self.focus == BundFocus::Marks && !self.marks.is_empty() {
            self.mark_highlight = self.mark_highlight.min(self.marks.len() - 1);
            if self.mode == Mode::Search {
                if let Some(prompt) = &mut self.prompt {
                    prompt.phase = Phase::Navigating;
                }
            }
        }
    }

    pub fn is_marked(&self, law: &LawRef) -> bool {
        let key = law.shortcut.to_lowercase();
        self.marks.iter().any(|mark| mark == &key)
    }

    pub fn marked_laws(&self, index: &[LawRef]) -> Vec<LawRef> {
        self.marks
            .iter()
            .filter_map(|key| {
                self.visible
                    .iter()
                    .chain(index.iter())
                    .find(|law| law.shortcut.eq_ignore_ascii_case(key))
                    .cloned()
            })
            .collect()
    }

    pub fn show_all(&mut self, index: &[LawRef]) {
        self.visible = index.to_vec();
        self.highlight = 0;
    }

    pub fn enter_search(&mut self, index: &[LawRef]) {
        if self.mode == Mode::Search {
            self.resume_typing();
            return;
        }
        self.prompt = Some(SearchPrompt::enter());
        self.mode = Mode::Search;
        self.apply_search(index);
    }

    pub fn resume_typing(&mut self) {
        if let Some(prompt) = &mut self.prompt {
            prompt.resume_typing();
        }
    }

    pub fn apply_search(&mut self, index: &[LawRef]) {
        let query = self.search_query().to_string();
        let keep = self.visible.get(self.highlight).map(|law| law.slug.clone());
        self.visible = filter_name(index, &query)
            .into_iter()
            .cloned()
            .collect();
        self.highlight = keep
            .and_then(|slug| self.visible.iter().position(|law| law.slug == slug))
            .unwrap_or(0);
        if self.visible.is_empty() {
            self.highlight = 0;
        } else {
            self.highlight = self.highlight.min(self.visible.len() - 1);
        }
    }

    pub fn type_search(&mut self, ch: char, index: &[LawRef]) {
        if let Some(prompt) = &mut self.prompt {
            prompt.type_char(ch);
        }
        self.apply_search(index);
    }

    pub fn search_backspace(&mut self, index: &[LawRef]) {
        if let Some(prompt) = &mut self.prompt {
            prompt.backspace();
        }
        self.apply_search(index);
    }

    pub fn lock_search(&mut self) -> bool {
        let count = self.visible.len();
        self.prompt.as_mut().is_some_and(|p| p.lock(count))
    }

    pub fn leave_search(&mut self, index: &[LawRef]) {
        self.prompt = None;
        self.mode = Mode::Normal;
        self.show_all(index);
    }

    pub fn move_highlight(&mut self, delta: isize) {
        if self.in_marks() {
            let max = self.marks.len().saturating_sub(1) as isize;
            let next = self.mark_highlight as isize + delta;
            self.mark_highlight = next.clamp(0, max) as usize;
            return;
        }
        if self.visible.is_empty() {
            return;
        }
        let next = self.highlight as isize + delta;
        self.highlight = next.clamp(0, self.visible.len() as isize - 1) as usize;
    }

    pub fn toggle_mark(&mut self) {
        let key = if self.in_marks() {
            self.marks.get(self.mark_highlight).cloned()
        } else {
            self.highlighted().map(|law| law.shortcut.to_lowercase())
        };
        let Some(key) = key else {
            return;
        };
        if let Some(index) = self.marks.iter().position(|mark| mark == &key) {
            self.marks.remove(index);
            if self.marks.is_empty() {
                self.focus = BundFocus::Catalog;
                self.mark_highlight = 0;
            } else {
                self.mark_highlight = self.mark_highlight.min(self.marks.len() - 1);
            }
        } else {
            self.marks.push(key);
        }
    }

    pub fn marked_list(&self) -> Vec<String> {
        self.marks.clone()
    }
}

pub const IDENTIFIER_WIDTH: usize = 10;
const MARK_WIDTH: usize = 2;
const SLUG_GAP: usize = 2;

pub fn title_indent() -> usize {
    MARK_WIDTH + IDENTIFIER_WIDTH + SLUG_GAP
}

pub fn slug_gap() -> usize {
    SLUG_GAP
}

pub fn wrap_title(title: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for word in title.split_whitespace() {
        for piece in char_chunks(word, width) {
            let piece_len = piece.chars().count();
            if current.is_empty() {
                current = piece;
                current_len = piece_len;
                continue;
            }
            if current_len + 1 + piece_len <= width {
                current.push(' ');
                current.push_str(&piece);
                current_len += 1 + piece_len;
            } else {
                lines.push(std::mem::take(&mut current));
                current = piece;
                current_len = piece_len;
            }
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

pub fn wrap_slug(slug: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = String::new();
    for part in split_before_capitals(slug) {
        if current.is_empty() {
            current = part;
            continue;
        }
        if current.chars().count() + part.chars().count() <= width {
            current.push_str(&part);
        } else {
            lines.push(std::mem::take(&mut current));
            current = part;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

fn split_before_capitals(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    let mut parts = Vec::new();
    let mut start = 0;
    for i in 1..chars.len() {
        if !chars[i].is_uppercase() {
            continue;
        }
        let prev = chars[i - 1];
        let camel = prev.is_lowercase();
        let acronym = prev.is_uppercase()
            && i + 1 < chars.len()
            && chars[i + 1].is_lowercase();
        if camel || acronym {
            parts.push(chars[start..i].iter().collect());
            start = i;
        }
    }
    parts.push(chars[start..].iter().collect());
    parts
}

fn char_chunks(word: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() <= width {
        return vec![word.to_string()];
    }
    chars
        .chunks(width)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::LawRef;

    fn sample_index() -> Vec<LawRef> {
        vec![
            LawRef::new("AAG", "aag", "Ausgleich", &[]),
            LawRef::new("BGB", "bgb", "Bürgerliches Gesetzbuch", &[]),
            LawRef::new("StVG", "stvg", "Straßenverkehrsgesetz", &[]),
            LawRef::new("StGB", "stgb", "Strafgesetzbuch", &[]),
        ]
    }

    #[test]
    fn open_shows_entire_catalog() {
        let overlay = BundesrechtOverlay::new(&sample_index());
        assert_eq!(overlay.visible.len(), 4);
        assert_eq!(overlay.search_display(), "/ Suche");
    }

    #[test]
    fn slash_search_is_substring_across_all() {
        let mut overlay = BundesrechtOverlay::new(&sample_index());
        overlay.enter_search(&sample_index());
        overlay.type_search('s', &sample_index());
        overlay.type_search('t', &sample_index());
        overlay.type_search('r', &sample_index());
        overlay.type_search('a', &sample_index());
        overlay.type_search('ß', &sample_index());
        assert_eq!(overlay.visible.len(), 1);
        assert_eq!(overlay.visible[0].shortcut, "StVG");
        assert_eq!(overlay.search_display(), "/straß");
    }

    #[test]
    fn space_toggles_multiple_marks() {
        let mut overlay = BundesrechtOverlay::new(&sample_index());
        overlay.highlight = 2;
        overlay.toggle_mark();
        overlay.move_highlight(1);
        overlay.toggle_mark();
        let mut marks = overlay.marked_list();
        marks.sort();
        assert_eq!(marks, vec!["stgb", "stvg"]);
        overlay.toggle_mark();
        assert_eq!(overlay.marked_list(), vec!["stvg"]);
    }

    #[test]
    fn marked_list_ignores_search_and_tab_toggles_focus() {
        let index = sample_index();
        let mut overlay = BundesrechtOverlay::new(&index);
        overlay.highlight = 1;
        overlay.toggle_mark();
        overlay.highlight = 2;
        overlay.toggle_mark();
        assert_eq!(overlay.marked_list(), vec!["bgb", "stvg"]);
        overlay.enter_search(&index);
        overlay.type_search('s', &index);
        overlay.type_search('t', &index);
        overlay.type_search('g', &index);
        overlay.type_search('b', &index);
        assert_eq!(overlay.visible.len(), 1);
        assert_eq!(overlay.visible[0].shortcut, "StGB");
        assert_eq!(
            overlay
                .marked_laws(&index)
                .iter()
                .map(|law| law.shortcut.as_str())
                .collect::<Vec<_>>(),
            vec!["BGB", "StVG"]
        );
        assert_eq!(overlay.focus, BundFocus::Catalog);
        overlay.toggle_focus();
        assert_eq!(overlay.focus, BundFocus::Marks);
        assert_eq!(overlay.mark_highlight, 0);
        overlay.move_highlight(1);
        overlay.toggle_mark();
        assert_eq!(overlay.marked_list(), vec!["bgb"]);
        overlay.toggle_mark();
        assert!(overlay.marks.is_empty());
        assert_eq!(overlay.focus, BundFocus::Catalog);
    }

    #[test]
    fn esc_from_search_restores_full_catalog() {
        let mut overlay = BundesrechtOverlay::new(&sample_index());
        overlay.enter_search(&sample_index());
        overlay.type_search('b', &sample_index());
        overlay.leave_search(&sample_index());
        assert_eq!(overlay.mode, Mode::Normal);
        assert_eq!(overlay.visible.len(), 4);
        assert_eq!(overlay.search_display(), "/ Suche");
    }

    #[test]
    fn wrap_title_breaks_on_spaces_then_chars() {
        assert_eq!(
            wrap_title("Bürgerliches Gesetzbuch", 12),
            vec!["Bürgerliches", "Gesetzbuch"]
        );
        assert_eq!(
            wrap_title("Grundgesetz für die Bundesrepublik Deutschland", 20),
            vec!["Grundgesetz für die", "Bundesrepublik", "Deutschland"]
        );
        assert_eq!(wrap_title("Straßenverkehrsgesetz", 8), vec!["Straßenv", "erkehrsg", "esetz"]);
        assert_eq!(wrap_title("", 10), vec![""]);
    }

    #[test]
    fn wrap_slug_splits_before_capitals() {
        assert_eq!(
            wrap_slug("BAAZustVExtraLong", IDENTIFIER_WIDTH),
            vec!["BAAZustV", "ExtraLong"]
        );
        assert_eq!(wrap_slug("BGB", IDENTIFIER_WIDTH), vec!["BGB"]);
        assert_eq!(wrap_slug("StVG", IDENTIFIER_WIDTH), vec!["StVG"]);
        assert_eq!(wrap_slug("BVerfGG", IDENTIFIER_WIDTH), vec!["BVerfGG"]);
        assert_eq!(
            split_before_capitals("BAAZustVExtraLong"),
            vec!["BAA", "Zust", "V", "Extra", "Long"]
        );
    }
}
