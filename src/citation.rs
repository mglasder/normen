use std::sync::LazyLock;

use regex::Regex;

use crate::models::CitationKey;

static QUERY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\A(?:§§?|art(?:ikel)?)?\s*(\d+)\s*([a-zäöü])?\s*\z").unwrap()
});
static FIRST_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(?:§§?|art(?:ikel)?)\s*(\d+)\s*([a-zäöü])?").unwrap());
static RANGE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)§§\s*(\d+)\s*bis\s*(\d+)").unwrap());
static ART_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bArt(?:ikel)?\.?\s+").unwrap());

impl CitationKey {
    pub fn parse_query(raw: &str) -> Option<Self> {
        key_from_caps(QUERY_RE.captures(raw.trim())?)
    }

    pub fn parse_first(raw: &str) -> Option<Self> {
        key_from_caps(FIRST_RE.captures(raw)?)
    }
}

pub fn parse_range(raw: &str) -> Option<(i32, i32)> {
    let caps = RANGE_RE.captures(raw)?;
    let start = caps[1].parse().ok()?;
    let end = caps[2].parse().ok()?;
    Some((start, end))
}

pub fn normalize_art(citation: &str) -> String {
    ART_RE.replace_all(citation, "Art. ").into_owned()
}

fn key_from_caps(caps: regex::Captures<'_>) -> Option<CitationKey> {
    let number = caps[1].parse().ok()?;
    let suffix = caps
        .get(2)
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_default();
    Some(CitationKey::with_suffix(number, suffix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_query_accepts_number_and_letter() {
        assert_eq!(
            CitationKey::parse_query("31a"),
            Some(CitationKey::with_suffix(31, "a"))
        );
        assert_eq!(
            CitationKey::parse_query("§ 433"),
            Some(CitationKey::new(433))
        );
        assert_eq!(CitationKey::parse_query("Art 1"), Some(CitationKey::new(1)));
        assert_eq!(
            CitationKey::parse_query("artikel 20a"),
            Some(CitationKey::with_suffix(20, "a"))
        );
        assert!(CitationKey::parse_query("Kaufvertrag").is_none());
    }

    #[test]
    fn parse_first_finds_enbez_inside_label() {
        assert_eq!(CitationKey::parse_first("§ 31a").unwrap().number, 31);
        assert_eq!(CitationKey::parse_first("§ 31a").unwrap().suffix, "a");
    }

    #[test]
    fn parse_range_covers_inclusive_span() {
        assert_eq!(parse_range("§§ 3 bis 6"), Some((3, 6)));
        assert!(parse_range("§ 433").is_none());
    }

    #[test]
    fn normalize_art_inserts_period() {
        assert_eq!(normalize_art("Art 74"), "Art. 74");
        assert_eq!(normalize_art("Artikel 1"), "Art. 1");
    }
}
