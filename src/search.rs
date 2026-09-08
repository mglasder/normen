use std::sync::LazyLock;

use regex::Regex;

use crate::document::format_citation;
use crate::models::{CitationKey, Law, Norm, SearchHit};

const MIN_FUZZY_SCORE: i64 = 70;
const PREVIEW_WIDTH: usize = 80;
const HIGHLIGHT_STYLE: &str = "bold #10171e on #f5d595";

static QUERY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\A(?:§§?|art(?:ikel)?)?\s*(\d+)\s*([a-zäöü])?\s*\z").unwrap()
});

pub fn parse_citation_query(query: &str) -> Option<CitationKey> {
    let caps = QUERY_RE.captures(query.trim())?;
    let number = caps[1].parse().ok()?;
    let suffix = caps
        .get(2)
        .map(|m| m.as_str().to_lowercase())
        .unwrap_or_default();
    Some(CitationKey::with_suffix(number, suffix))
}

pub fn lookup_norm<'a>(law: &'a Law, query: &str) -> Option<&'a Norm> {
    if let Some(key) = parse_citation_query(query) {
        return law.norms.iter().find(|norm| norm.keys.contains(&key));
    }
    let citation = query.trim();
    law.norms.iter().find(|norm| norm.citation == citation)
}

pub fn search_norms(law: &Law, query: &str) -> Vec<SearchHit> {
    let needle = query.trim();
    if needle.is_empty() {
        return Vec::new();
    }
    let exact = if parse_citation_query(needle).is_some() {
        lookup_norm(law, needle).cloned()
    } else {
        None
    };
    let folded = needle.to_lowercase();
    let mut hits: Vec<SearchHit> = Vec::new();
    for norm in &law.norms {
        let title_blob = format!("{} {}", norm.citation, norm.title);
        let title_score = text_score(needle, &title_blob);
        let body_score = text_score(needle, &norm.text);
        let in_title = title_blob.to_lowercase().contains(&folded);
        let in_body = norm.text.to_lowercase().contains(&folded);
        let score = title_score.max(body_score);
        if !(in_title || in_body || score >= MIN_FUZZY_SCORE) {
            continue;
        }
        hits.push(SearchHit {
            norm: norm.clone(),
            in_title: in_title || title_score >= body_score,
            preview: make_preview(norm, needle),
            score: score as f64,
        });
    }
    if let Some(exact) = exact {
        hits.retain(|hit| hit.norm.citation != exact.citation);
        let rest_sort = {
            let mut rest = hits;
            rest.sort_by_key(|hit| norm_number_key(&hit.norm));
            rest
        };
        let mut out = vec![SearchHit {
            norm: exact.clone(),
            in_title: true,
            preview: make_preview(&exact, needle),
            score: 100.0,
        }];
        out.extend(rest_sort);
        return out;
    }
    hits.sort_by_key(|hit| norm_number_key(&hit.norm));
    hits
}

fn text_score(needle: &str, text: &str) -> i64 {
    let needle = needle.to_lowercase();
    let text = text.to_lowercase();
    if needle.is_empty() {
        return 0;
    }
    if text.contains(&needle) {
        return 100;
    }
    let mut best = 0i64;
    for word in text.split(|ch: char| !ch.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }
        best = best.max(word_match_score(&needle, word));
    }
    best
}

fn word_match_score(needle: &str, word: &str) -> i64 {
    if word == needle {
        return 100;
    }
    if word.starts_with(needle) {
        return 90;
    }
    let needle_len = needle.chars().count();
    let word_len = word.chars().count();
    if needle_len < 3 || word_len < 3 {
        return 0;
    }
    let slack = (needle_len / 3).max(2);
    if word_len.abs_diff(needle_len) > slack {
        return 0;
    }
    levenshtein_ratio(needle, word)
}

fn lower_char(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

fn levenshtein_ratio(left: &str, right: &str) -> i64 {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let max = left.len().max(right.len());
    if max == 0 {
        return 100;
    }
    let dist = levenshtein(&left, &right);
    (100.0 * (1.0 - dist as f64 / max as f64)).round() as i64
}

fn levenshtein(left: &[char], right: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=right.len()).collect();
    let mut curr = vec![0; right.len() + 1];
    for (i, &a) in left.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &b) in right.iter().enumerate() {
            let cost = usize::from(a != b);
            curr[j + 1] = (prev[j + 1] + 1)
                .min(curr[j] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[right.len()]
}

fn norm_number_key(norm: &Norm) -> (u8, i32, String) {
    match norm.keys.first() {
        Some(first) => (0, first.number, first.suffix.clone()),
        None => (1, 0, norm.citation.clone()),
    }
}

pub fn make_preview(norm: &Norm, query: &str) -> String {
    let text: String = if norm.text.is_empty() {
        String::new()
    } else {
        norm.text.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    if text.is_empty() {
        return norm.title.clone();
    }
    let chars: Vec<char> = text.chars().collect();
    let start = match_char_index(&chars, query)
        .unwrap_or(0)
        .saturating_sub(20);
    preview_snippet(&chars, start)
}

fn preview_snippet(chars: &[char], start: usize) -> String {
    let end = (start + PREVIEW_WIDTH).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    if start > 0 {
        out = format!("…{out}");
    }
    if end < chars.len() {
        out = format!("{out}…");
    }
    out
}

fn match_char_index(chars: &[char], query: &str) -> Option<usize> {
    find_ignore_case(chars, query).or_else(|| closest_word_start(chars, query))
}

fn find_ignore_case(chars: &[char], query: &str) -> Option<usize> {
    let needle: Vec<char> = query.chars().map(lower_char).collect();
    if needle.is_empty() || chars.len() < needle.len() {
        return None;
    }
    chars.windows(needle.len()).position(|window| {
        window
            .iter()
            .copied()
            .map(lower_char)
            .eq(needle.iter().copied())
    })
}

fn closest_word_start(chars: &[char], query: &str) -> Option<usize> {
    closest_word_range(chars, query).map(|(start, _)| start)
}

fn closest_word_range(chars: &[char], query: &str) -> Option<(usize, usize)> {
    let needle: String = query.trim().chars().map(lower_char).collect();
    if needle.is_empty() {
        return None;
    }
    let mut best: Option<(i64, usize, usize)> = None;
    let mut index = 0;
    while index < chars.len() {
        if !chars[index].is_alphanumeric() {
            index += 1;
            continue;
        }
        let start = index;
        while index < chars.len() && chars[index].is_alphanumeric() {
            index += 1;
        }
        let word: String = chars[start..index].iter().copied().map(lower_char).collect();
        let score = word_match_score(&needle, &word);
        if score >= MIN_FUZZY_SCORE && best.is_none_or(|(best_score, _, _)| score > best_score) {
            best = Some((score, start, index));
        }
    }
    best.map(|(_, start, end)| (start, end))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub start: usize,
    pub end: usize,
    pub style: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledText {
    pub plain: String,
    pub spans: Vec<TextSpan>,
}

impl StyledText {
    fn new() -> Self {
        Self {
            plain: String::new(),
            spans: Vec::new(),
        }
    }

    fn append(&mut self, text: &str, style: Option<&str>) {
        let start = self.plain.len();
        self.plain.push_str(text);
        if let Some(style) = style {
            self.spans.push(TextSpan {
                start,
                end: self.plain.len(),
                style: style.to_string(),
            });
        }
    }

    fn append_styled(&mut self, other: StyledText) {
        let offset = self.plain.len();
        self.plain.push_str(&other.plain);
        for span in other.spans {
            self.spans.push(TextSpan {
                start: span.start + offset,
                end: span.end + offset,
                style: span.style,
            });
        }
    }
}

pub fn format_search_hit(hit: &SearchHit, query: &str, abbreviation: &str) -> StyledText {
    let mut prompt = StyledText::new();
    let citation = format_citation(&hit.norm.citation, abbreviation);
    append_marked(&mut prompt, &citation, query, Some("bold"));
    if !hit.norm.title.is_empty() {
        prompt.append("  ", None);
        prompt.append_styled(highlight_text(&hit.norm.title, query));
    }
    let preview = hit.preview.trim();
    if !preview.is_empty() && preview != hit.norm.title {
        prompt.append("\n", None);
        prompt.append_styled(highlight_text(preview, query));
    }
    prompt
}

fn append_marked(prompt: &mut StyledText, text: &str, query: &str, fallback: Option<&str>) {
    let marked = highlight_text(text, query);
    let mut pos = 0;
    for span in &marked.spans {
        if span.start > pos {
            prompt.append(&marked.plain[pos..span.start], fallback);
        }
        prompt.append(&marked.plain[span.start..span.end], Some(&span.style));
        pos = span.end;
    }
    if pos < marked.plain.len() {
        prompt.append(&marked.plain[pos..], fallback);
    }
}

pub fn highlight_text(text: &str, query: &str) -> StyledText {
    let mut result = StyledText::new();
    let needle: Vec<char> = query.trim().chars().map(lower_char).collect();
    let chars: Vec<char> = text.chars().collect();
    if needle.is_empty() || chars.is_empty() {
        result.append(text, None);
        return result;
    }
    let mut ranges = exact_highlight_ranges(&chars, &needle);
    if ranges.is_empty() {
        if let Some(range) = closest_word_range(&chars, query) {
            ranges.push(range);
        }
    }
    let mut pos = 0;
    for (start, end) in ranges {
        if start > pos {
            result.append(&chars_to_string(&chars[pos..start]), None);
        }
        result.append(&chars_to_string(&chars[start..end]), Some(HIGHLIGHT_STYLE));
        pos = end;
    }
    if pos < chars.len() {
        result.append(&chars_to_string(&chars[pos..]), None);
    }
    result
}

fn chars_to_string(chars: &[char]) -> String {
    chars.iter().collect()
}

fn exact_highlight_ranges(chars: &[char], needle: &[char]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    if needle.is_empty() || chars.len() < needle.len() {
        return ranges;
    }
    let mut index = 0;
    while index + needle.len() <= chars.len() {
        let matched = chars[index..index + needle.len()]
            .iter()
            .copied()
            .map(lower_char)
            .eq(needle.iter().copied());
        if matched {
            let (start, end) = word_bounds(chars, index);
            if ranges.last().is_none_or(|(_, prev_end)| start >= *prev_end) {
                ranges.push((start, end));
            }
            index = end;
        } else {
            index += 1;
        }
    }
    ranges
}

fn word_bounds(chars: &[char], index: usize) -> (usize, usize) {
    let mut start = index;
    while start > 0 && chars[start - 1].is_alphanumeric() {
        start -= 1;
    }
    let mut end = index;
    while end < chars.len() && chars[end].is_alphanumeric() {
        end += 1;
    }
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CitationKey;
    use crate::parser::parse_law_xml;

    const SAMPLE: &[u8] = include_bytes!("../tests/fixtures/sample.xml");
    const GG_SAMPLE: &[u8] = include_bytes!("../tests/fixtures/gg_sample.xml");

    fn bgb() -> Law {
        parse_law_xml(SAMPLE)
    }

    fn gg() -> Law {
        parse_law_xml(GG_SAMPLE)
    }

    #[test]
    fn lookup_by_bare_number() {
        assert_eq!(lookup_norm(&bgb(), "1").unwrap().citation, "§ 1");
        assert_eq!(lookup_norm(&bgb(), "433").unwrap().citation, "§ 433");
    }

    #[test]
    fn lookup_accepts_paragraph_prefix_and_letter_suffix() {
        assert_eq!(lookup_norm(&bgb(), "§ 31a").unwrap().citation, "§ 31a");
        assert_eq!(lookup_norm(&bgb(), "31A").unwrap().citation, "§ 31a");
    }

    #[test]
    fn lookup_maps_repealed_range() {
        assert_eq!(
            lookup_norm(&bgb(), "4").unwrap().citation,
            "(XXXX) §§ 3 bis 6"
        );
    }

    #[test]
    fn lookup_gg_article() {
        assert_eq!(lookup_norm(&gg(), "1").unwrap().citation, "Art 1");
        assert_eq!(lookup_norm(&gg(), "art 20").unwrap().citation, "Art 20");
    }

    #[test]
    fn lookup_unknown_number_returns_none() {
        assert!(lookup_norm(&bgb(), "9999").is_none());
    }

    #[test]
    fn full_text_search_finds_title_and_body() {
        let hits = search_norms(&bgb(), "Kaufvertrag");
        assert_eq!(
            hits.iter()
                .map(|hit| hit.norm.citation.as_str())
                .collect::<Vec<_>>(),
            vec!["§ 433"]
        );
        assert!(hits[0].in_title);
    }

    #[test]
    fn full_text_search_is_case_insensitive() {
        let hits = search_norms(&gg(), "würde");
        assert_eq!(hits[0].norm.citation, "Art 1");
        assert!(!hits[0].in_title);
    }

    #[test]
    fn search_citation_query_ranks_exact_lettered_match_first() {
        let law = Law {
            abbreviation: "StGB".into(),
            title: "Test".into(),
            norms: vec![
                Norm {
                    citation: "§ 113".into(),
                    title: "Widerstand".into(),
                    text: "widerstand gegen vollstreckungsbeamte".into(),
                    keys: vec![CitationKey::new(113)],
                },
                Norm {
                    citation: "§ 113a".into(),
                    title: "Tätlicher Angriff".into(),
                    text: "taetlicher angriff auf vollstreckungsbeamte".into(),
                    keys: vec![CitationKey::with_suffix(113, "a")],
                },
            ],
        };
        let hits = search_norms(&law, "113a");
        assert_eq!(hits[0].norm.citation, "§ 113a");
    }

    #[test]
    fn search_results_sorted_by_norm_number() {
        let law = Law {
            abbreviation: "BGB".into(),
            title: "Test".into(),
            norms: vec![
                Norm {
                    citation: "§ 433".into(),
                    title: "Kauf".into(),
                    text: "text".into(),
                    keys: vec![CitationKey::new(433)],
                },
                Norm {
                    citation: "§ 1".into(),
                    title: "Beginn".into(),
                    text: "kauf im text".into(),
                    keys: vec![CitationKey::new(1)],
                },
                Norm {
                    citation: "§ 31a".into(),
                    title: "Haftung".into(),
                    text: "auch kauf".into(),
                    keys: vec![CitationKey::with_suffix(31, "a")],
                },
            ],
        };
        let hits = search_norms(&law, "kauf");
        assert_eq!(
            hits.iter()
                .map(|hit| hit.norm.citation.as_str())
                .collect::<Vec<_>>(),
            vec!["§ 1", "§ 31a", "§ 433"]
        );
    }

    #[test]
    fn fuzzy_search_finds_partial_title() {
        let hits = search_norms(&bgb(), "kaufvertr");
        assert_eq!(hits[0].norm.citation, "§ 433");
        assert!(
            hits[0].preview.contains("Kaufvertrag") || hits[0].norm.title.contains("Kaufvertrag")
        );
    }

    #[test]
    fn search_does_not_match_unrelated_norms_by_letter_subsequence() {
        let law = Law {
            abbreviation: "BGB".into(),
            title: "Test".into(),
            norms: vec![
                Norm {
                    citation: "§ 1".into(),
                    title: "Beginn der Rechtsfähigkeit".into(),
                    text: "Die Verwaltung des Antrags auf Eintragung in das Register.".into(),
                    keys: vec![CitationKey::new(1)],
                },
                Norm {
                    citation: "§ 433".into(),
                    title: "Kaufvertrag".into(),
                    text: "Durch den Kaufvertrag wird der Verkäufer verpflichtet.".into(),
                    keys: vec![CitationKey::new(433)],
                },
            ],
        };
        let hits = search_norms(&law, "Vertrag");
        assert_eq!(
            hits.iter()
                .map(|hit| hit.norm.citation.as_str())
                .collect::<Vec<_>>(),
            vec!["§ 433"],
            "subsequence letters in Verwaltung/Antrag must not count as Vertrag"
        );
    }

    #[test]
    fn search_hits_mention_the_query_or_a_close_word() {
        let hits = search_norms(&bgb(), "Kaufvertrag");
        assert!(!hits.is_empty());
        for hit in &hits {
            let blob = format!(
                "{} {} {}",
                hit.norm.citation, hit.norm.title, hit.norm.text
            )
            .to_lowercase();
            assert!(
                blob.contains("kaufvertrag"),
                "{} matched Kaufvertrag without containing it (preview {:?})",
                hit.norm.citation,
                hit.preview
            );
        }
    }

    fn preview_norm(text: &str) -> Norm {
        Norm {
            citation: "§ 1".into(),
            title: "Titel".into(),
            text: text.into(),
            keys: vec![CitationKey::new(1)],
        }
    }

    #[test]
    fn preview_windows_around_match_after_umlauts() {
        let prefix = "Übertragung für regelmäßig gültige Änderung. ".repeat(8);
        let text = format!("{prefix}Der Kaufvertrag verpflichtet den Verkäufer.");
        let preview = make_preview(&preview_norm(&text), "Kaufvertrag");
        assert!(
            preview.contains("Kaufvertrag"),
            "preview should include the match, got {preview:?}"
        );
    }

    #[test]
    fn preview_windows_around_closest_word_when_query_is_not_exact() {
        let prefix = "Übertragung für regelmäßig gültige Änderung. ".repeat(8);
        let text = format!("{prefix}Der Kaufvertrag verpflichtet den Verkäufer.");
        let preview = make_preview(&preview_norm(&text), "kaufvertr");
        assert!(
            preview.contains("Kaufvertrag"),
            "preview should include the closest word, got {preview:?}"
        );
    }

    #[test]
    fn search_hit_includes_preview() {
        let hits = search_norms(&bgb(), "Kaufvertrag");
        assert!(!hits[0].preview.is_empty());
        assert!(
            hits[0].preview.contains("Kaufvertrag") || hits[0].norm.title.contains("Kaufvertrag")
        );
    }

    #[test]
    fn search_hit_prompt_bolds_citation_and_separates_preview() {
        let hits = search_norms(&bgb(), "Kaufvertrag");
        let prompt = format_search_hit(&hits[0], "Kaufvertrag", "BGB");
        let lines: Vec<&str> = prompt.plain.lines().collect();
        assert!(lines[0].starts_with("§ 433 BGB"));
        assert!(prompt.plain.contains("Kaufvertrag"));
        let citation_end = "§ 433 BGB".len();
        assert!(prompt.spans.iter().any(|span| {
            span.start == 0 && span.end == citation_end && span.style.contains("bold")
        }));
        assert!(lines.len() >= 2);
    }

    #[test]
    fn highlight_text_marks_query() {
        let rendered = highlight_text("Vertragstypische Pflichten beim Kaufvertrag", "Kauf");
        assert!(rendered.plain.contains("Kaufvertrag"));
        assert!(rendered
            .spans
            .iter()
            .any(|span| span.style.contains("f5d595")));
        assert!(rendered
            .spans
            .iter()
            .any(|span| span.style.to_lowercase().contains("on #")));
    }

    fn highlighted_piece(text: &StyledText) -> String {
        text.spans
            .iter()
            .find(|span| span.style.contains("f5d595"))
            .map(|span| text.plain[span.start..span.end].to_string())
            .unwrap_or_default()
    }

    #[test]
    fn highlight_text_marks_query_after_umlauts() {
        let rendered = highlight_text("Übertragung für regelmäßig gültige Änderung am Kaufvertrag", "Kaufvertrag");
        assert_eq!(highlighted_piece(&rendered), "Kaufvertrag");
    }

    #[test]
    fn highlight_text_marks_closest_word_when_query_is_not_exact() {
        let rendered = highlight_text("Der Kaufvertrag verpflichtet den Verkäufer.", "kaufvertr");
        assert_eq!(highlighted_piece(&rendered), "Kaufvertrag");
        let fuzzy = highlight_text("Beginn der Rechtsfähigkeit", "Rechtsfahigkeit");
        assert_eq!(highlighted_piece(&fuzzy), "Rechtsfähigkeit");
    }

    #[test]
    fn format_search_hit_highlights_citation_and_preview() {
        let law = bgb();
        let hits = search_norms(&law, "433");
        let prompt = format_search_hit(&hits[0], "433", "BGB");
        assert!(
            prompt
                .spans
                .iter()
                .any(|span| span.style.contains("f5d595")
                    && prompt.plain[span.start..span.end].contains("433")),
            "citation number should be highlighted: {prompt:?}"
        );
    }
}
