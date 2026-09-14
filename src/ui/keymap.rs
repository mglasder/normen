use std::collections::HashMap;

use crossterm::event::{self, KeyCode, KeyEventKind, KeyModifiers};

use crate::config::Config;

use super::Key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Prefix,
    Menu,
    Reader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    MoveLeft,
    MoveDown,
    MoveUp,
    MoveRight,
    ParagraphPrev,
    ParagraphNext,
    GotoTop,
    GotoBottom,
    PageDown,
    PageUp,
    EnterSearch,
    EnterPara,
    EnterNormal,
    Confirm,
    Help,
    Quit,
    TabPrefix,
    TabNext,
    TabPrev,
    TabClose,
    TabMenu,
    Bundesrecht,
    RemoveCore,
    MarksList,
}

pub struct Keymap {
    inner: HashMap<String, String>,
}

const PREFIX_ACTIONS: &[(&str, Action)] = &[
    ("tab_next", Action::TabNext),
    ("tab_prev", Action::TabPrev),
    ("tab_close", Action::TabClose),
    ("tab_menu", Action::TabMenu),
    ("tab_bundesrecht", Action::Bundesrecht),
];

const MENU_ACTIONS: &[(&str, Action)] = &[
    ("move_down", Action::MoveDown),
    ("move_up", Action::MoveUp),
    ("move_left", Action::MoveLeft),
    ("move_right", Action::MoveRight),
    ("enter_search", Action::EnterSearch),
    ("enter_para", Action::EnterPara),
    ("confirm", Action::Confirm),
    ("enter_normal", Action::EnterNormal),
    ("help", Action::Help),
    ("quit", Action::Quit),
    ("tab_prefix", Action::TabPrefix),
    ("core_remove", Action::RemoveCore),
    ("marks_list", Action::MarksList),
];

const READER_ACTIONS: &[(&str, Action)] = &[
    ("move_down", Action::MoveDown),
    ("move_up", Action::MoveUp),
    ("move_left", Action::MoveLeft),
    ("move_right", Action::MoveRight),
    ("enter_search", Action::EnterSearch),
    ("enter_para", Action::EnterPara),
    ("confirm", Action::Confirm),
    ("enter_normal", Action::EnterNormal),
    ("help", Action::Help),
    ("quit", Action::Quit),
    ("tab_prefix", Action::TabPrefix),
    ("paragraph_next", Action::ParagraphNext),
    ("paragraph_prev", Action::ParagraphPrev),
    ("goto_top", Action::GotoTop),
    ("goto_bottom", Action::GotoBottom),
    ("page_down", Action::PageDown),
    ("page_up", Action::PageUp),
];

impl Keymap {
    pub fn from_config(config: &Config) -> Self {
        Self {
            inner: config.keymap(),
        }
    }

    pub fn binding(&self, name: &str) -> &str {
        self.inner.get(name).map(String::as_str).unwrap_or("")
    }

    pub fn resolve(&self, key: Key, ctx: Context) -> Option<Action> {
        let names = match ctx {
            Context::Prefix => PREFIX_ACTIONS,
            Context::Menu => MENU_ACTIONS,
            Context::Reader => READER_ACTIONS,
        };
        names
            .iter()
            .copied()
            .find(|&(name, _)| key_matches(key, self.binding(name)))
            .map(|(_, action)| action)
    }
}

pub fn pretty_binding(binding: &str) -> String {
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
            "tab" => "Tab",
            other => other,
        };
        out.push_str(pretty);
    }
    out
}

pub(crate) fn key_matches(key: Key, binding: &str) -> bool {
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
        Key::Tab => part == "tab",
    }
}

pub fn map_crossterm(event: event::KeyEvent) -> Option<Key> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    let ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
    match event.code {
        KeyCode::Char(c) if ctrl => Some(Key::Ctrl(c.to_ascii_lowercase())),
        KeyCode::Char('\t') => Some(Key::Tab),
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
        KeyCode::Tab | KeyCode::BackTab => Some(Key::Tab),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::ui::Key;

    fn keymap(text: &str) -> Keymap {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(&path, text).unwrap();
        Keymap::from_config(&Config::new(&path))
    }

    #[test]
    fn default_j_is_move_down_in_reader() {
        let km = keymap("");
        assert_eq!(km.resolve(Key::Char('j'), Context::Reader), Some(Action::MoveDown));
        assert_eq!(km.resolve(Key::Char('J'), Context::Reader), Some(Action::ParagraphNext));
    }

    #[test]
    fn remapped_paragraph_does_not_keep_shift_j() {
        let km = keymap("[reader]\nparagraph_next = y\nparagraph_prev = x\n");
        assert_eq!(km.resolve(Key::Char('y'), Context::Reader), Some(Action::ParagraphNext));
        assert_eq!(km.resolve(Key::Char('J'), Context::Reader), None);
    }

    #[test]
    fn prefix_n_is_tab_next_not_in_reader() {
        let km = keymap("");
        assert_eq!(km.resolve(Key::Char('n'), Context::Prefix), Some(Action::TabNext));
        assert_eq!(km.resolve(Key::Char('n'), Context::Reader), None);
    }

    #[test]
    fn prefix_a_is_bundesrecht() {
        let km = keymap("");
        assert_eq!(
            km.resolve(Key::Char('a'), Context::Prefix),
            Some(Action::Bundesrecht)
        );
        assert_eq!(km.resolve(Key::Char('d'), Context::Menu), Some(Action::RemoveCore));
        assert_eq!(km.resolve(Key::Char('j'), Context::Menu), Some(Action::MoveDown));
        assert_eq!(km.resolve(Key::Tab, Context::Menu), Some(Action::MarksList));
    }

    #[test]
    fn unmodified_quit_from_conf_stays_ctrl_q() {
        let km = keymap("[picker]\nquit = q\n");
        assert_eq!(km.resolve(Key::Char('q'), Context::Reader), None);
        assert_eq!(km.resolve(Key::Ctrl('q'), Context::Reader), Some(Action::Quit));
    }

    #[test]
    fn pretty_binding_shows_letters_not_shift_chords() {
        assert_eq!(pretty_binding("shift+n"), "N");
        assert_eq!(pretty_binding("K,shift+k"), "K");
        assert_eq!(pretty_binding("j,down"), "j");
        assert_eq!(pretty_binding("ctrl+d"), "Ctrl-d");
        assert_eq!(pretty_binding("escape"), "Esc");
        assert_eq!(pretty_binding("tab"), "Tab");
        assert_eq!(pretty_binding("enter"), "Enter");
    }

    #[test]
    fn shift_j_keyevent_maps_to_uppercase() {
        let event = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::SHIFT,
        );
        assert_eq!(map_crossterm(event), Some(Key::Char('J')));
        let plain = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('j'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(map_crossterm(plain), Some(Key::Char('j')));
        let tab = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(map_crossterm(tab), Some(Key::Tab));
        let tab_char = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('\t'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(map_crossterm(tab_char), Some(Key::Tab));
    }
}
