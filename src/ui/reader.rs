use crate::catalog::LawRef;
use crate::document::law_pos_label;
use crate::models::{Law, SearchHit};
use crate::search::{lookup_norm, parse_citation_query, search_norms};

use super::{window_range, Mode, WINDOW_RADIUS};

pub struct ReaderTab {
    pub law_ref: LawRef,
    pub law: Law,
    pub current: usize,
    pub mode: Mode,
    pub para: String,
    pub cmd: String,
    pub search_nav: bool,
    pub hits: Vec<SearchHit>,
    pub hit_highlight: usize,
    pub hit_index: i32,
    pub view_start: usize,
    pub skip_lines: usize,
    pub search_origin: usize,
}

impl ReaderTab {
    pub fn from_law(law_ref: LawRef, law: Law) -> Self {
        let mut tab = Self {
            law_ref,
            law,
            current: 0,
            mode: Mode::Normal,
            para: String::new(),
            cmd: String::new(),
            search_nav: false,
            hits: Vec::new(),
            hit_highlight: 0,
            hit_index: -1,
            view_start: 0,
            skip_lines: 0,
            search_origin: 0,
        };
        if tab.law.norms.is_empty() {
            tab.current = 0;
        }
        tab
    }

    pub fn shortcut(&self) -> &str {
        self.law_ref.shortcut
    }

    pub fn current_citation(&self) -> Option<&str> {
        self.law
            .norms
            .get(self.current)
            .map(|norm| norm.citation.as_str())
    }

    pub fn pos_label(&self) -> String {
        let Some(current) = self.law.norms.get(self.current) else {
            return String::new();
        };
        law_pos_label(&self.law, current)
    }

    pub fn mounted_len(&self) -> usize {
        let (start, end) = window_range(self.current, self.law.norms.len(), WINDOW_RADIUS);
        end.saturating_sub(start)
    }

    pub fn apply_initial(&mut self, query: Option<&str>) {
        let Some(raw) = query.map(str::trim).filter(|q| !q.is_empty()) else {
            return;
        };
        if let Some(needle) = raw.strip_prefix('/') {
            self.enter_search();
            self.cmd = needle.to_string();
            self.render_results();
            return;
        }
        self.run_query(raw);
    }

    pub fn enter_para(&mut self, seed: &str) {
        self.para = seed.to_string();
        self.mode = Mode::Para;
    }

    pub fn enter_search(&mut self) {
        if self.mode == Mode::Search {
            self.search_nav = false;
            return;
        }
        self.search_nav = false;
        self.cmd.clear();
        self.hits.clear();
        self.hit_highlight = 0;
        self.mode = Mode::Search;
    }

    pub fn leave_search(&mut self) {
        self.search_nav = false;
        self.cmd.clear();
        self.mode = Mode::Normal;
    }

    pub fn enter_normal(&mut self) {
        self.para.clear();
        self.cmd.clear();
        self.search_nav = false;
        self.mode = Mode::Normal;
    }

    pub fn type_search(&mut self, ch: char) {
        self.cmd.push(ch);
        self.render_results();
    }

    pub fn search_backspace(&mut self) {
        self.cmd.pop();
        self.render_results();
    }

    pub fn render_results(&mut self) {
        let query = self.cmd.trim_start_matches('/');
        let keep = self
            .hits
            .get(self.hit_highlight)
            .map(|hit| hit.norm.citation.clone());
        if query.trim().is_empty() {
            self.hits.clear();
            self.hit_highlight = 0;
            self.search_origin = 0;
            return;
        }
        self.hits = search_norms(&self.law, query);
        let exact = if parse_citation_query(query).is_some() {
            lookup_norm(&self.law, query).map(|norm| norm.citation.clone())
        } else {
            None
        };
        self.hit_highlight = 0;
        if let Some(citation) = exact {
            if let Some(index) = self
                .hits
                .iter()
                .position(|hit| hit.norm.citation == citation)
            {
                self.hit_highlight = index;
            }
        } else if let Some(keep) = keep {
            if let Some(index) = self.hits.iter().position(|hit| hit.norm.citation == keep) {
                self.hit_highlight = index;
            }
        }
        self.search_origin = 0;
    }

    pub fn lock_search(&mut self) -> bool {
        if self.hits.is_empty() {
            self.leave_search();
            return false;
        }
        self.search_nav = true;
        true
    }

    pub fn move_hit(&mut self, delta: isize, width: u16, view_height: u16) {
        if self.hits.is_empty() {
            return;
        }
        let next = self.hit_highlight as isize + delta;
        self.hit_highlight = next.clamp(0, self.hits.len() as isize - 1) as usize;
        self.scroll_hit_into_view(width, view_height);
    }

    pub fn ensure_hit_visible(&mut self, width: u16, view_height: u16) {
        if self.hits.is_empty() {
            self.search_origin = 0;
            return;
        }
        self.hit_highlight = self.hit_highlight.min(self.hits.len() - 1);
        self.scroll_hit_into_view(width, view_height);
    }

    fn hit_span(&self, index: usize, width: u16) -> usize {
        let query = self.cmd.trim_start_matches('/');
        self.hits
            .get(index)
            .map(|hit| {
                (super::search_card_height(hit, query, &self.law.abbreviation, width) as usize)
                    .saturating_add(1)
            })
            .unwrap_or(1)
            .max(1)
    }

    fn hit_y_in_view(&self, index: usize, width: u16) -> Option<(isize, usize)> {
        if index < self.search_origin {
            return None;
        }
        let mut y = 0isize;
        for i in self.search_origin..=index {
            let height = self.hit_span(i, width);
            if i == index {
                return Some((y, height));
            }
            y += height as isize;
        }
        None
    }

    fn pin_hit_top(&mut self) {
        self.search_origin = self.hit_highlight;
    }

    fn pin_hit_bottom(&mut self, width: u16, view_height: usize) {
        let height = self.hit_span(self.hit_highlight, width);
        if height >= view_height {
            self.pin_hit_top();
            return;
        }
        let mut used = height;
        let mut start = self.hit_highlight;
        while start > 0 {
            let prev = start - 1;
            let prev_height = self.hit_span(prev, width);
            if used + prev_height <= view_height {
                used += prev_height;
                start = prev;
                continue;
            }
            break;
        }
        self.search_origin = start;
    }

    fn scroll_hit_into_view(&mut self, width: u16, view_height: u16) {
        let view_height = view_height.max(1) as usize;
        match self.hit_y_in_view(self.hit_highlight, width) {
            Some((y, height)) if y >= 0 && y + height as isize <= view_height as isize => {}
            Some((y, _)) if y < 0 => self.pin_hit_top(),
            Some(_) => self.pin_hit_bottom(width, view_height),
            None if self.hit_highlight < self.search_origin => self.pin_hit_top(),
            None => self.pin_hit_bottom(width, view_height),
        }
    }

    pub fn open_highlighted_hit(&mut self) {
        let Some(hit) = self.hits.get(self.hit_highlight).cloned() else {
            self.leave_search();
            return;
        };
        self.hit_index = self.hit_highlight as i32;
        self.select_citation(&hit.norm.citation);
        self.leave_search();
    }

    pub fn confirm_para(&mut self) {
        let query = self.para.trim().to_string();
        self.para.clear();
        self.mode = Mode::Normal;
        if !query.is_empty() {
            self.run_query(&query);
        }
    }

    pub fn run_query(&mut self, raw: &str) {
        let query = raw.trim();
        if query.is_empty() {
            return;
        }
        if let Some(needle) = query.strip_prefix('/') {
            self.enter_search();
            self.cmd = needle.to_string();
            self.render_results();
            return;
        }
        if let Some(norm) = lookup_norm(&self.law, query) {
            let citation = norm.citation.clone();
            self.hits.clear();
            self.hit_index = -1;
            self.select_citation(&citation);
        }
    }

    pub fn select_citation(&mut self, citation: &str) {
        if let Some(index) = self
            .law
            .norms
            .iter()
            .position(|norm| norm.citation == citation)
        {
            self.current = index;
            self.pin_top();
        }
    }

    pub fn step_norm(&mut self, delta: isize, width: u16, view_height: u16) {
        if self.law.norms.is_empty() {
            return;
        }
        let next = self.current as isize + delta;
        if next < 0 || next >= self.law.norms.len() as isize {
            return;
        }
        self.current = next as usize;
        self.scroll_current_into_view(width, view_height);
    }

    pub fn scroll_by(&mut self, delta: isize, width: u16, view_height: u16) {
        if delta > 0 {
            for _ in 0..delta {
                self.scroll_down(width);
            }
            if !self.current_overlaps_view(width, view_height) {
                self.current = self.view_start;
            }
        } else {
            for _ in 0..(-delta) {
                self.scroll_up(width);
            }
            if !self.current_overlaps_view(width, view_height) {
                self.current = self.last_visible_index(width, view_height);
            }
        }
    }

    fn span_at(&self, index: usize, width: u16) -> usize {
        self.law
            .norms
            .get(index)
            .map(|norm| super::card_span(norm, &self.law.abbreviation, width))
            .unwrap_or(1)
            .max(1)
    }

    fn pin_top(&mut self) {
        self.view_start = self.current;
        self.skip_lines = 0;
    }

    fn pin_bottom(&mut self, width: u16, view_height: usize) {
        let height = self.span_at(self.current, width);
        if height >= view_height {
            self.pin_top();
            return;
        }
        let mut used = height;
        let mut start = self.current;
        let mut skip = 0;
        while start > 0 {
            let prev = start - 1;
            let prev_height = self.span_at(prev, width);
            if used + prev_height <= view_height {
                used += prev_height;
                start = prev;
                continue;
            }
            skip = used + prev_height - view_height;
            start = prev;
            break;
        }
        self.view_start = start;
        self.skip_lines = skip;
    }

    fn current_overlaps_view(&self, width: u16, view_height: u16) -> bool {
        let view_height = view_height.max(1) as isize;
        match self.card_y_in_view(self.current, width) {
            Some((y, height)) => y < view_height && y + height as isize > 0,
            None => false,
        }
    }

    fn last_visible_index(&self, width: u16, view_height: u16) -> usize {
        let view_height = view_height.max(1) as isize;
        let mut y = -(self.skip_lines as isize);
        let mut last = self.view_start;
        for i in self.view_start..self.law.norms.len() {
            if y >= view_height {
                break;
            }
            last = i;
            y += self.span_at(i, width) as isize;
        }
        last
    }

    fn card_y_in_view(&self, index: usize, width: u16) -> Option<(isize, usize)> {
        if index < self.view_start {
            return None;
        }
        let mut y = -(self.skip_lines as isize);
        for i in self.view_start..=index {
            let height = self.span_at(i, width);
            if i == index {
                return Some((y, height));
            }
            y += height as isize;
        }
        None
    }

    fn scroll_current_into_view(&mut self, width: u16, view_height: u16) {
        let view_height = view_height.max(1) as usize;
        match self.card_y_in_view(self.current, width) {
            Some((y, height)) if y >= 0 && y + height as isize <= view_height as isize => {}
            Some((y, _)) if y < 0 => self.pin_top(),
            Some(_) => self.pin_bottom(width, view_height),
            None if self.current < self.view_start => self.pin_top(),
            None => self.pin_bottom(width, view_height),
        }
    }

    fn scroll_down(&mut self, width: u16) {
        if self.law.norms.is_empty() {
            return;
        }
        self.skip_lines += 1;
        loop {
            let last = self.law.norms.len() - 1;
            let span = self.span_at(self.view_start, width);
            if self.view_start >= last || self.skip_lines < span {
                if self.view_start == last {
                    self.skip_lines = self.skip_lines.min(span.saturating_sub(1));
                }
                break;
            }
            self.skip_lines -= span;
            self.view_start += 1;
        }
    }

    fn scroll_up(&mut self, width: u16) {
        if self.skip_lines > 0 {
            self.skip_lines -= 1;
            return;
        }
        if self.view_start == 0 {
            return;
        }
        self.view_start -= 1;
        self.skip_lines = self.span_at(self.view_start, width).saturating_sub(1);
    }

    pub fn goto_top(&mut self) {
        if !self.law.norms.is_empty() {
            self.current = 0;
            self.pin_top();
        }
    }

    pub fn goto_bottom(&mut self) {
        if !self.law.norms.is_empty() {
            self.current = self.law.norms.len() - 1;
            self.pin_top();
        }
    }

    pub fn type_digit(&mut self, digit: char) {
        if self.mode == Mode::Para {
            self.para.push(digit);
        } else {
            self.enter_para(&digit.to_string());
        }
    }

    pub fn type_para_char(&mut self, ch: char) {
        if self.para.chars().any(|c| c.is_ascii_digit()) {
            self.para.push(ch);
        }
    }
}
