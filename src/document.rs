use std::sync::LazyLock;

use regex::Regex;

use crate::citation::normalize_art;
use crate::models::{CitationKey, Law, Norm};

static LIST_ITEM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(\d+[a-zäöü]?\.|[a-zäöü]\))\s+(.*)$").unwrap());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyBlock {
    pub kind: String,
    pub text: String,
    pub marker: String,
}

impl BodyBlock {
    fn prose(text: impl Into<String>) -> Self {
        Self {
            kind: "prose".into(),
            text: text.into(),
            marker: String::new(),
        }
    }

    fn list(marker: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            kind: "list".into(),
            text: text.into(),
            marker: marker.into(),
        }
    }
}

pub fn format_citation(citation: &str, abbreviation: &str) -> String {
    let text = normalize_art(citation);
    if !abbreviation.is_empty()
        && !text.to_lowercase().contains(&abbreviation.to_lowercase())
        && (text.contains('§') || text.starts_with("Art."))
    {
        return format!("{text} {abbreviation}");
    }
    text
}

pub fn norm_heading(norm: &Norm, abbreviation: &str) -> String {
    let heading = format_citation(&norm.citation, abbreviation);
    if !norm.title.is_empty() {
        format!("{heading}  {}", norm.title)
    } else {
        heading
    }
}

pub fn format_norm(norm: &Norm, abbreviation: &str) -> String {
    let body = if norm.text.is_empty() {
        "(kein Text)"
    } else {
        &norm.text
    };
    format!("{}\n\n{body}", norm_heading(norm, abbreviation))
}

pub fn format_law(law: &Law) -> String {
    law.norms
        .iter()
        .map(|norm| format_norm(norm, &law.abbreviation))
        .collect::<Vec<_>>()
        .join("\n\n\n")
}

fn key_label(key: &CitationKey) -> String {
    format!("{}{}", key.number, key.suffix)
}

pub fn norm_number_label(norm: &Norm) -> String {
    if norm.keys.len() > 1 {
        return format!(
            "{}-{}",
            key_label(&norm.keys[0]),
            key_label(norm.keys.last().unwrap())
        );
    }
    if let Some(key) = norm.keys.first() {
        return key_label(key);
    }
    if let Some(key) = CitationKey::parse_first(&norm.citation) {
        return key_label(&key);
    }
    norm.citation.clone()
}

pub fn law_last_number_label(law: &Law) -> String {
    for norm in law.norms.iter().rev() {
        if !norm.keys.is_empty() {
            return key_label(norm.keys.last().unwrap());
        }
    }
    law.norms.len().to_string()
}

fn numbered_label(norm: &Norm) -> Option<String> {
    if norm.keys.is_empty() {
        None
    } else {
        Some(norm_number_label(norm))
    }
}

pub fn law_pos_current_label(law: &Law, current: &Norm) -> String {
    if let Some(numbered) = numbered_label(current) {
        return numbered;
    }
    let index = law
        .norms
        .iter()
        .position(|norm| norm.citation == current.citation)
        .unwrap_or(0);
    for norm in &law.norms[index + 1..] {
        if let Some(label) = numbered_label(norm) {
            return label;
        }
    }
    for norm in law.norms[..index].iter().rev() {
        if let Some(label) = numbered_label(norm) {
            return label;
        }
    }
    norm_number_label(current)
}

pub fn law_pos_label(law: &Law, current: &Norm) -> String {
    format!(
        "{}:{}",
        law_pos_current_label(law, current),
        law_last_number_label(law)
    )
}

pub fn iter_body_blocks(text: &str) -> Vec<BodyBlock> {
    if text.trim().is_empty() {
        return vec![BodyBlock::prose("(kein Text)")];
    }
    let mut blocks = Vec::new();
    for paragraph in text.split("\n\n") {
        let mut prose: Vec<&str> = Vec::new();
        for line in paragraph.split('\n') {
            if let Some(caps) = LIST_ITEM_RE.captures(line) {
                if !prose.is_empty() {
                    blocks.push(BodyBlock::prose(prose.join("\n")));
                    prose.clear();
                }
                blocks.push(BodyBlock::list(&caps[1], &caps[2]));
            } else {
                prose.push(line);
            }
        }
        if !prose.is_empty() {
            blocks.push(BodyBlock::prose(prose.join("\n")));
        }
    }
    if blocks.is_empty() {
        vec![BodyBlock::prose("(kein Text)")]
    } else {
        blocks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_law_xml;

    const SAMPLE: &[u8] = include_bytes!("../tests/fixtures/sample.xml");
    const GG_SAMPLE: &[u8] = include_bytes!("../tests/fixtures/gg_sample.xml");

    #[test]
    fn norm_heading_is_citation_and_title() {
        let law = parse_law_xml(SAMPLE);
        let first = law
            .norms
            .iter()
            .find(|norm| norm.citation == "§ 1")
            .unwrap();
        assert_eq!(
            norm_heading(first, "BGB"),
            "§ 1 BGB  Beginn der Rechtsfähigkeit"
        );
        let book = law
            .norms
            .iter()
            .find(|norm| norm.citation == "Buch 1")
            .unwrap();
        assert_eq!(norm_heading(book, "BGB"), "Buch 1  Allgemeiner Teil");
    }

    #[test]
    fn format_norm_includes_citation_title_and_text() {
        let law = parse_law_xml(SAMPLE);
        let first = law
            .norms
            .iter()
            .find(|norm| norm.citation == "§ 1")
            .unwrap();
        let rendered = format_norm(first, "BGB");
        assert!(rendered.starts_with("§ 1 BGB  Beginn der Rechtsfähigkeit"));
        assert!(rendered.contains("Rechtsfähigkeit des Menschen"));
    }

    #[test]
    fn format_law_contains_every_norm() {
        let law = parse_law_xml(SAMPLE);
        let rendered = format_law(&law);
        assert!(rendered.contains("Buch 1  Allgemeiner Teil"));
        assert!(rendered.contains("§ 1 BGB  Beginn der Rechtsfähigkeit"));
        assert!(rendered.contains("§ 433 BGB  Vertragstypische Pflichten beim Kaufvertrag"));
        assert!(rendered.contains("Kaufpreis zu zahlen"));
    }

    #[test]
    fn norm_number_label_keeps_letter_suffix() {
        let law = parse_law_xml(SAMPLE);
        let numbered: Vec<_> = law
            .norms
            .iter()
            .map(|norm| (norm.citation.as_str(), norm_number_label(norm)))
            .collect();
        let get = |citation: &str| {
            numbered
                .iter()
                .find(|(c, _)| *c == citation)
                .map(|(_, l)| l.as_str())
                .unwrap()
        };
        assert_eq!(get("§ 1"), "1");
        assert_eq!(get("§ 31a"), "31a");
        assert_eq!(get("§ 433"), "433");
        assert_eq!(get("(XXXX) §§ 3 bis 6"), "3-6");
    }

    #[test]
    fn status_pos_uses_named_numbers_not_list_index() {
        let law = parse_law_xml(SAMPLE);
        let lettered = law
            .norms
            .iter()
            .find(|norm| norm.citation == "§ 31a")
            .unwrap();
        assert_ne!(
            law.norms
                .iter()
                .position(|n| n.citation == "§ 31a")
                .unwrap()
                + 1,
            31
        );
        assert_eq!(law_pos_label(&law, lettered), "31a:433");
    }

    #[test]
    fn status_pos_skips_trailing_anhang() {
        let law = parse_law_xml(GG_SAMPLE);
        let art74 = law
            .norms
            .iter()
            .find(|norm| norm.citation == "Art 74")
            .unwrap();
        assert_eq!(law.norms.last().unwrap().citation, "Anhang EV");
        assert_eq!(law_pos_label(&law, art74), "74:74");
    }

    #[test]
    fn status_pos_ignores_section_headers() {
        let law = parse_law_xml(SAMPLE);
        let book = law
            .norms
            .iter()
            .find(|norm| norm.citation == "Buch 1")
            .unwrap();
        assert_eq!(law_pos_label(&law, book), "1:433");
        let gg = parse_law_xml(GG_SAMPLE);
        let section = gg.norms.iter().find(|norm| norm.citation == "I.").unwrap();
        assert_eq!(law_pos_label(&gg, section), "1:74");
        assert_eq!(law_pos_label(&gg, gg.norms.last().unwrap()), "74:74");
    }

    #[test]
    fn gg_citation_uses_art_period_and_abbreviation() {
        let law = parse_law_xml(GG_SAMPLE);
        let art74 = law
            .norms
            .iter()
            .find(|norm| norm.citation == "Art 74")
            .unwrap();
        assert_eq!(format_citation("Art 74", "GG"), "Art. 74 GG");
        assert_eq!(norm_heading(art74, "GG"), "Art. 74 GG");
    }

    #[test]
    fn body_blocks_split_numbered_items() {
        let law = parse_law_xml(GG_SAMPLE);
        let art74 = law
            .norms
            .iter()
            .find(|norm| norm.citation == "Art 74")
            .unwrap();
        let blocks = iter_body_blocks(&art74.text);
        let items: Vec<_> = blocks.iter().filter(|b| b.kind == "list").collect();
        assert_eq!(items[0].marker, "1.");
        assert!(items[0].text.starts_with("das bürgerliche Recht"));
        assert_eq!(items[1].marker, "2.");
        assert_eq!(items[2].marker, "19a.");
        let prose: Vec<_> = blocks
            .iter()
            .filter(|b| b.kind == "prose")
            .map(|b| b.text.as_str())
            .collect();
        assert!(prose.iter().any(|text| text.contains("folgende Gebiete:")));
        assert!(prose.iter().any(|text| text.contains("Bundesgesetz")));
    }
}
