use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use ini::Ini;

pub fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("normen")
        .join("normen.conf")
}

pub const DEFAULT_TEXT: &str = "\
# normen — configuration
# All settings live here. Unknown keys are ignored so this file can grow.

[keys]
enter_para = i
enter_normal = escape
move_left = h,left
move_down = j,down
move_up = k,up
move_right = l,right
confirm = enter
help = ?
quit = ctrl+q

[reader]
enter_search = /
paragraph_prev = K,shift+k
paragraph_next = J,shift+j
goto_top = g
goto_bottom = G
next_hit = n
prev_hit = shift+n
page_down = ctrl+d
page_up = ctrl+u

[tabs]
tab_prefix = ctrl+n
tab_next = n
tab_prev = p
tab_close = x
tab_menu = m
";

pub fn default_keys() -> HashMap<String, String> {
    HashMap::from([
        ("enter_para".into(), "i".into()),
        ("enter_normal".into(), "escape".into()),
        ("move_left".into(), "h,left".into()),
        ("move_down".into(), "j,down".into()),
        ("move_up".into(), "k,up".into()),
        ("move_right".into(), "l,right".into()),
        ("confirm".into(), "enter".into()),
        ("help".into(), "?".into()),
        ("enter_search".into(), "/".into()),
        ("paragraph_prev".into(), "K,shift+k".into()),
        ("paragraph_next".into(), "J,shift+j".into()),
        ("goto_top".into(), "g".into()),
        ("goto_bottom".into(), "G".into()),
        ("next_hit".into(), "n".into()),
        ("prev_hit".into(), "shift+n".into()),
        ("page_down".into(), "ctrl+d".into()),
        ("page_up".into(), "ctrl+u".into()),
        ("tab_prefix".into(), "ctrl+n".into()),
        ("tab_next".into(), "n".into()),
        ("tab_prev".into(), "p".into()),
        ("tab_close".into(), "x".into()),
        ("tab_menu".into(), "m".into()),
        ("quit".into(), "ctrl+q".into()),
    ])
}

pub struct Config {
    pub path: PathBuf,
    ini: Ini,
}

impl Config {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let ini = load_ini(&path);
        Self { path, ini }
    }

    pub fn load(&mut self) {
        self.ini = load_ini(&self.path);
    }

    pub fn ensure_file(&self) {
        if self.path.exists() {
            return;
        }
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&self.path, DEFAULT_TEXT);
    }

    pub fn get(&self, section: &str, option: &str, fallback: Option<&str>) -> Option<String> {
        match self.ini.get_from(Some(section), option) {
            Some(value) => {
                let cleaned = clean(value);
                if cleaned.is_empty() {
                    fallback.map(str::to_string)
                } else {
                    Some(cleaned)
                }
            }
            None => fallback.map(str::to_string),
        }
    }

    pub fn section(&self, name: &str) -> HashMap<String, String> {
        let Some(props) = self.ini.section(Some(name)) else {
            return HashMap::new();
        };
        props
            .iter()
            .map(|(key, value)| (key.to_string(), clean(value)))
            .filter(|(_, value)| !value.is_empty())
            .collect()
    }

    pub fn keymap(&self) -> HashMap<String, String> {
        let mut resolved = default_keys();
        let mut overlay = HashMap::new();
        for name in ["keys", "picker", "reader", "tabs"] {
            overlay.extend(self.section(name));
        }
        if overlay.contains_key("enter_insert") && !overlay.contains_key("enter_para") {
            if let Some(value) = overlay.get("enter_insert").cloned() {
                overlay.insert("enter_para".into(), value);
            }
        }
        resolved.extend(overlay);
        let prefix = resolved
            .get("tab_prefix")
            .cloned()
            .unwrap_or_else(|| "ctrl+n".into());
        if !has_modifier(&prefix) {
            resolved.insert("tab_prefix".into(), default_keys()["tab_prefix"].clone());
        }
        let quit_key = resolved
            .get("quit")
            .cloned()
            .unwrap_or_else(|| "ctrl+q".into());
        if !has_modifier(&quit_key) {
            resolved.insert("quit".into(), default_keys()["quit"].clone());
        }
        let defaults = default_keys();
        for name in ["tab_next", "tab_prev", "tab_close", "tab_menu"] {
            let value = resolved.get(name).cloned().unwrap_or_default();
            resolved.insert(name.into(), tab_suffix(&value, &defaults[name]));
        }
        let enter_para = resolved
            .get("enter_para")
            .cloned()
            .unwrap_or_else(|| "i".into());
        resolved.insert("enter_insert".into(), enter_para);
        let mut para_keys = Vec::new();
        for name in ["paragraph_prev", "paragraph_next"] {
            if let Some(value) = resolved.get(name) {
                para_keys.extend(
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|key| !key.is_empty())
                        .map(str::to_string),
                );
            }
        }
        for name in ["move_down", "move_up"] {
            if let Some(value) = resolved.get(name).cloned() {
                let kept: Vec<&str> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|key| !key.is_empty() && !para_keys.iter().any(|p| p == key))
                    .collect();
                if !kept.is_empty() {
                    resolved.insert(name.into(), kept.join(","));
                }
            }
        }
        resolved
    }
}

fn load_ini(path: &Path) -> Ini {
    if !path.exists() {
        return Ini::new();
    }
    Ini::load_from_file(path).unwrap_or_else(|_| Ini::new())
}

fn has_modifier(binding: &str) -> bool {
    binding
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .any(|part| part.contains('+'))
}

fn tab_suffix(value: &str, default: &str) -> String {
    let mut parts = Vec::new();
    for part in value.split(',') {
        let mut key = part.trim();
        if key.is_empty() {
            continue;
        }
        if key.contains('+') {
            key = key.rsplit('+').next().unwrap_or(key);
        }
        if !key.is_empty() {
            parts.push(key.to_string());
        }
    }
    if parts.is_empty() {
        default.to_string()
    } else {
        parts.join(",")
    }
}

fn clean(value: &str) -> String {
    value.split('#').next().unwrap_or("").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::new(dir.path().join("normen.conf"));
        let keys = config.keymap();
        assert_eq!(keys.get("paragraph_prev").unwrap(), "K,shift+k");
        assert_eq!(keys.get("paragraph_next").unwrap(), "J,shift+j");
        assert_eq!(keys.get("tab_prefix").unwrap(), "ctrl+n");
        assert_eq!(keys.get("tab_next").unwrap(), "n");
        assert_eq!(keys.get("tab_prev").unwrap(), "p");
        assert_eq!(keys.get("tab_close").unwrap(), "x");
        assert_eq!(keys.get("tab_menu").unwrap(), "m");
        assert_eq!(keys.get("quit").unwrap(), "ctrl+q");
        assert_eq!(
            keys.get("move_left").unwrap(),
            default_keys().get("move_left").unwrap()
        );
    }

    #[test]
    fn file_overrides_reader_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(&path, "[reader]\nparagraph_prev = x\nparagraph_next = y\n").unwrap();
        let keys = Config::new(&path).keymap();
        assert_eq!(keys.get("paragraph_prev").unwrap(), "x");
        assert_eq!(keys.get("paragraph_next").unwrap(), "y");
        assert_eq!(keys.get("enter_para").unwrap(), "i");
    }

    #[test]
    fn shared_keys_section_overrides_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(&path, "[keys]\nmove_down = d\n").unwrap();
        assert_eq!(Config::new(&path).keymap().get("move_down").unwrap(), "d");
    }

    #[test]
    fn get_reads_future_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(&path, "[display]\nruler = on\n").unwrap();
        let config = Config::new(&path);
        assert_eq!(config.get("display", "ruler", None).as_deref(), Some("on"));
        assert_eq!(
            config.get("display", "missing", Some("off")).as_deref(),
            Some("off")
        );
    }

    #[test]
    fn paragraph_keys_are_removed_from_line_scroll() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(
            &path,
            "[keys]\nmove_down = j,J,down\nmove_up = k,K,up\n[reader]\nparagraph_prev = K\nparagraph_next = J\n",
        )
        .unwrap();
        let keys = Config::new(&path).keymap();
        let down: Vec<_> = keys.get("move_down").unwrap().split(',').collect();
        let up: Vec<_> = keys.get("move_up").unwrap().split(',').collect();
        assert!(!down.contains(&"J"));
        assert!(!up.contains(&"K"));
        assert_eq!(keys.get("paragraph_next").unwrap(), "J");
        assert_eq!(keys.get("paragraph_prev").unwrap(), "K");
    }

    #[test]
    fn tab_suffixes_may_be_unmodified() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(
            &path,
            "[tabs]\ntab_next = n\ntab_prev = p\ntab_close = x\ntab_menu = m\n",
        )
        .unwrap();
        let keys = Config::new(&path).keymap();
        assert_eq!(keys.get("tab_prefix").unwrap(), "ctrl+n");
        assert_eq!(keys.get("tab_next").unwrap(), "n");
        assert_eq!(keys.get("tab_prev").unwrap(), "p");
        assert_eq!(keys.get("tab_close").unwrap(), "x");
        assert_eq!(keys.get("tab_menu").unwrap(), "m");
    }

    #[test]
    fn chord_style_tab_suffixes_are_normalized() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(
            &path,
            "[tabs]\ntab_next = ctrl+n\ntab_prev = ctrl+p\ntab_close = ctrl+x\ntab_menu = ctrl+m\n",
        )
        .unwrap();
        let keys = Config::new(&path).keymap();
        assert_eq!(keys.get("tab_next").unwrap(), "n");
        assert_eq!(keys.get("tab_prev").unwrap(), "p");
        assert_eq!(keys.get("tab_close").unwrap(), "x");
        assert_eq!(keys.get("tab_menu").unwrap(), "m");
    }

    #[test]
    fn stale_quit_without_modifier_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(&path, "[picker]\nquit = q\n").unwrap();
        assert_eq!(Config::new(&path).keymap().get("quit").unwrap(), "ctrl+q");
    }

    #[test]
    fn modified_tab_keys_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(&path, "[tabs]\ntab_prefix = alt+n\n").unwrap();
        assert_eq!(
            Config::new(&path).keymap().get("tab_prefix").unwrap(),
            "alt+n"
        );
    }

    #[test]
    fn ensure_file_writes_defaults_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("normen.conf");
        let config = Config::new(&path);
        config.ensure_file();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), DEFAULT_TEXT);
        std::fs::write(&path, "[reader]\nback = Q\n").unwrap();
        config.ensure_file();
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("[reader]\nback = Q\n"));
    }
}
