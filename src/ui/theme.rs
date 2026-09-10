use ratatui::style::Color;

pub const PRIMARY: Color = Color::Rgb(0x9c, 0xe5, 0xc0);
pub const SECONDARY: Color = Color::Rgb(0xa3, 0xb8, 0xef);
pub const ACCENT: Color = Color::Rgb(0xf5, 0xd5, 0x95);
pub const FOREGROUND: Color = Color::Rgb(0xce, 0xd4, 0xdf);
pub const BACKGROUND: Color = Color::Rgb(0x10, 0x17, 0x1e);
pub const ERROR: Color = Color::Rgb(0xef, 0x88, 0x91);
pub const SURFACE: Color = Color::Rgb(0x13, 0x1a, 0x21);
pub const BORDER: Color = Color::Rgb(0x40, 0x47, 0x4e);
pub const HIGHLIGHT_BG: Color = Color::Rgb(0x2a, 0x31, 0x38);
pub const SEARCH_FG: Color = Color::Rgb(0x10, 0x17, 0x1e);

pub fn parse_hex(value: &str) -> Option<Color> {
    let value = value.trim();
    if value.len() != 7 || !value.starts_with('#') {
        return None;
    }
    let r = u8::from_str_radix(&value[1..3], 16).ok()?;
    let g = u8::from_str_radix(&value[3..5], 16).ok()?;
    let b = u8::from_str_radix(&value[5..7], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub foreground: Color,
    pub background: Color,
    pub error: Color,
    pub surface: Color,
    pub border: Color,
    pub highlight_bg: Color,
    pub search_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            primary: PRIMARY,
            secondary: SECONDARY,
            accent: ACCENT,
            foreground: FOREGROUND,
            background: BACKGROUND,
            error: ERROR,
            surface: SURFACE,
            border: BORDER,
            highlight_bg: HIGHLIGHT_BG,
            search_fg: SEARCH_FG,
        }
    }
}

impl Theme {
    pub fn from_config(config: &crate::config::Config) -> Self {
        let mut theme = Self::default();
        for (key, value) in config.section("theme") {
            let Some(color) = parse_hex(&value) else {
                continue;
            };
            match key.to_ascii_lowercase().as_str() {
                "primary" => theme.primary = color,
                "secondary" => theme.secondary = color,
                "accent" => theme.accent = color,
                "foreground" => theme.foreground = color,
                "background" => theme.background = color,
                "error" => theme.error = color,
                "surface" => theme.surface = color,
                "border" => theme.border = color,
                "highlight_bg" => theme.highlight_bg = color,
                "search_fg" => theme.search_fg = color,
                _ => {}
            }
        }
        theme
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex_accepts_rrggbb() {
        assert_eq!(parse_hex("#9ce5c0"), Some(Color::Rgb(0x9c, 0xe5, 0xc0)));
        assert_eq!(parse_hex("#FF0000"), Some(Color::Rgb(0xff, 0, 0)));
    }

    #[test]
    fn parse_hex_rejects_invalid() {
        assert_eq!(parse_hex("red"), None);
        assert_eq!(parse_hex("#fff"), None);
        assert_eq!(parse_hex("9ce5c0"), None);
        assert_eq!(parse_hex("#gggggg"), None);
        assert_eq!(parse_hex("#9ce5c0ff"), None);
        assert_eq!(parse_hex(""), None);
    }

    #[test]
    fn theme_defaults_match_compiled_palette() {
        let dir = tempfile::tempdir().unwrap();
        let theme = Theme::from_config(&crate::config::Config::new(
            dir.path().join("normen.conf"),
        ));
        assert_eq!(theme, Theme::default());
        assert_eq!(theme.accent, ACCENT);
    }

    #[test]
    fn theme_overlays_quoted_tokens_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(
            &path,
            "[theme]\nPRIMARY = \"#ff0000\"\naccent = \"#00ff00\"\nunknown = \"#0000ff\"\n",
        )
        .unwrap();
        let theme = Theme::from_config(&crate::config::Config::new(&path));
        assert_eq!(theme.primary, Color::Rgb(0xff, 0, 0));
        assert_eq!(theme.accent, Color::Rgb(0, 0xff, 0));
        assert_eq!(theme.secondary, SECONDARY);
    }

    #[test]
    fn theme_invalid_token_keeps_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("normen.conf");
        std::fs::write(
            &path,
            "[theme]\nprimary = notacolor\naccent = \"red\"\nerror = \"#fff\"\nforeground = \"#abcdef\"\n",
        )
        .unwrap();
        let theme = Theme::from_config(&crate::config::Config::new(&path));
        assert_eq!(theme.primary, PRIMARY);
        assert_eq!(theme.accent, ACCENT);
        assert_eq!(theme.error, ERROR);
        assert_eq!(theme.foreground, Color::Rgb(0xab, 0xcd, 0xef));
    }
}

