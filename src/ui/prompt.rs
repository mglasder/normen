#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Typing,
    Navigating,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPrompt {
    pub query: String,
    pub phase: Phase,
    pub highlight: usize,
}

impl SearchPrompt {
    pub fn enter() -> Self {
        Self {
            query: String::new(),
            phase: Phase::Typing,
            highlight: 0,
        }
    }

    pub fn type_char(&mut self, ch: char) {
        self.query.push(ch);
    }

    pub fn backspace(&mut self) {
        self.query.pop();
    }

    pub fn lock(&mut self, result_count: usize) -> bool {
        if result_count == 0 {
            return false;
        }
        self.phase = Phase::Navigating;
        true
    }

    pub fn resume_typing(&mut self) {
        self.phase = Phase::Typing;
    }

    pub fn move_highlight(&mut self, delta: isize, result_count: usize) {
        if result_count == 0 {
            return;
        }
        let max = (result_count - 1) as isize;
        let next = (self.highlight as isize).saturating_add(delta);
        self.highlight = next.clamp(0, max) as usize;
    }

    pub fn reset(&mut self) {
        *self = Self::enter();
    }

    pub fn filter_query(&self) -> &str {
        self.query.trim_start_matches('/')
    }
}

pub const SEARCH_PLACEHOLDER: &str = "/ Suche";

pub fn search_line(searching: bool, query: &str) -> String {
    if searching {
        format!("/{query}")
    } else {
        SEARCH_PLACEHOLDER.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_with_hits_enters_navigating() {
        let mut p = SearchPrompt::enter();
        p.type_char('g');
        assert!(p.lock(2));
        assert_eq!(p.phase, Phase::Navigating);
    }

    #[test]
    fn lock_with_zero_hits_fails() {
        let mut p = SearchPrompt::enter();
        p.type_char('z');
        assert!(!p.lock(0));
        assert_eq!(p.phase, Phase::Typing);
    }

    #[test]
    fn slash_while_navigating_resumes_typing() {
        let mut p = SearchPrompt::enter();
        p.type_char('g');
        p.lock(1);
        p.resume_typing();
        assert_eq!(p.phase, Phase::Typing);
        assert_eq!(p.filter_query(), "g");
    }

    #[test]
    fn move_highlight_clamps() {
        let mut p = SearchPrompt::enter();
        p.lock(3);
        p.move_highlight(10, 3);
        assert_eq!(p.highlight, 2);
        p.move_highlight(-10, 3);
        assert_eq!(p.highlight, 0);
    }

    #[test]
    fn search_line_uses_german_placeholder() {
        assert_eq!(search_line(false, ""), "/ Suche");
        assert_eq!(search_line(true, "gg"), "/gg");
        assert_eq!(search_line(true, ""), "/");
    }
}
