use roxmltree::{Document, Node};

use crate::citation::parse_range;
use crate::models::{CitationKey, Law, Norm};

pub fn parse_law_xml(data: &[u8]) -> Law {
    try_parse_law_xml(data).expect("law xml should parse")
}

pub fn try_parse_law_xml(data: &[u8]) -> Result<Law, String> {
    let xml = strip_doctype(&String::from_utf8_lossy(data));
    let doc = Document::parse(&xml).map_err(|err| format!("XML: {err}"))?;
    let root = doc.root_element();
    let mut abbreviation = String::new();
    let mut title = String::new();
    let mut norms = Vec::new();

    for elem in root.children().filter(|n| n.has_tag_name("norm")) {
        let Some(meta) = child(elem, "metadaten") else {
            continue;
        };
        if abbreviation.is_empty() {
            abbreviation = child_text(meta, "jurabk");
        }
        if title.is_empty() {
            title = child_text(meta, "langue");
        }

        let enbez = child_text(meta, "enbez");
        if enbez.to_lowercase() == "inhaltsübersicht" {
            continue;
        }

        let (citation, titel, keys) = if !enbez.is_empty() {
            (
                enbez.clone(),
                collapsed(&child_text(meta, "titel")),
                citation_keys(&enbez),
            )
        } else {
            let Some((citation, titel)) = section_heading(meta) else {
                continue;
            };
            (citation, titel, Vec::new())
        };

        let text = norm_text(child(elem, "textdaten"));
        norms.push(Norm {
            citation,
            title: titel,
            text,
            keys,
        });
    }

    Ok(Law {
        abbreviation,
        title,
        norms,
    })
}

fn strip_doctype(xml: &str) -> String {
    if let Some(start) = xml.find("<!DOCTYPE") {
        if let Some(rel_end) = xml[start..].find('>') {
            let mut out = String::with_capacity(xml.len());
            out.push_str(&xml[..start]);
            out.push_str(&xml[start + rel_end + 1..]);
            return out;
        }
    }
    xml.to_string()
}

fn collapsed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn section_heading(meta: Node<'_, '_>) -> Option<(String, String)> {
    let unit = child(meta, "gliederungseinheit")?;
    let bez = collapsed(&child_text(unit, "gliederungsbez"));
    let titel = collapsed(&child_text(unit, "gliederungstitel"));
    if bez.is_empty() && titel.is_empty() {
        return None;
    }
    if bez.is_empty() {
        return Some((titel, String::new()));
    }
    Some((bez, titel))
}

fn citation_keys(enbez: &str) -> Vec<CitationKey> {
    if let Some((start, end)) = parse_range(enbez) {
        return (start..=end).map(CitationKey::new).collect();
    }
    CitationKey::parse_first(enbez).into_iter().collect()
}

fn norm_text(textdaten: Option<Node<'_, '_>>) -> String {
    let Some(textdaten) = textdaten else {
        return String::new();
    };
    let Some(text_elem) = child(textdaten, "text") else {
        return String::new();
    };
    let Some(content) = child(text_elem, "Content") else {
        return inner_text(text_elem).trim().to_string();
    };
    let paragraphs: Vec<String> = content
        .children()
        .filter(|n| n.has_tag_name("P"))
        .map(p_text)
        .filter(|piece| !piece.is_empty())
        .collect();
    paragraphs.join("\n\n")
}

fn p_text(paragraph: Node<'_, '_>) -> String {
    let mut parts = Vec::new();
    for child_node in paragraph.children() {
        if child_node.is_text() {
            let piece = child_node.text().unwrap_or("").trim();
            if !piece.is_empty() {
                parts.push(piece.to_string());
            }
        } else if child_node.has_tag_name("DL") {
            let listing = dl_text(child_node);
            if !listing.is_empty() {
                parts.push(listing);
            }
        } else if child_node.has_tag_name("BR") {
            parts.push("\n".into());
        } else if child_node.is_element() {
            let piece = inner_text(child_node);
            let piece = piece.trim();
            if !piece.is_empty() {
                parts.push(piece.to_string());
            }
        }
    }
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn dl_text(dl: Node<'_, '_>) -> String {
    let mut lines = Vec::new();
    let mut marker = String::new();
    for child_node in dl.children().filter(|n| n.is_element()) {
        if child_node.has_tag_name("DT") {
            marker = inner_text(child_node).trim().to_string();
            continue;
        }
        if !child_node.has_tag_name("DD") {
            continue;
        }
        let nested = child(child_node, "DL");
        let body = if nested.is_some() {
            p_text(child_node)
        } else {
            inner_text(child_node).trim().to_string()
        };
        if !marker.is_empty() && !body.is_empty() {
            lines.push(format!("{marker} {body}"));
        } else if !marker.is_empty() {
            lines.push(marker.clone());
        } else if !body.is_empty() {
            lines.push(body);
        }
        marker.clear();
    }
    lines.join("\n")
}

fn inner_text(node: Node<'_, '_>) -> String {
    let mut parts = Vec::new();
    walk(node, &mut parts);
    parts.concat()
}

fn walk(node: Node<'_, '_>, parts: &mut Vec<String>) {
    if node.has_tag_name("BR") {
        parts.push("\n".into());
    }
    for child_node in node.children() {
        if child_node.is_text() {
            if let Some(text) = child_node.text() {
                parts.push(text.to_string());
            }
        } else if child_node.is_element() {
            walk(child_node, parts);
        }
    }
}

fn child<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    node.children().find(|n| n.has_tag_name(tag))
}

fn child_text(node: Node<'_, '_>, tag: &str) -> String {
    child(node, tag)
        .map(|n| inner_text(n).trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = include_bytes!("../tests/fixtures/sample.xml");
    const GG_SAMPLE: &[u8] = include_bytes!("../tests/fixtures/gg_sample.xml");

    #[test]
    fn parse_law_metadata_and_numbered_norms() {
        let law = parse_law_xml(SAMPLE);

        assert_eq!(law.abbreviation, "BGB");
        assert_eq!(law.title, "Bürgerliches Gesetzbuch");
        assert_eq!(
            law.norms
                .iter()
                .map(|norm| norm.citation.as_str())
                .collect::<Vec<_>>(),
            vec!["Buch 1", "§ 1", "(XXXX) §§ 3 bis 6", "§ 31a", "§ 433"]
        );
        assert_eq!(law.norms[0].title, "Allgemeiner Teil");
        assert!(law.norms[0].keys.is_empty());
        assert_eq!(law.norms[0].text, "");
    }

    #[test]
    fn parse_norm_title_and_paragraph_text() {
        let law = parse_law_xml(SAMPLE);
        let first = law
            .norms
            .iter()
            .find(|norm| norm.citation == "§ 1")
            .expect("§ 1");

        assert_eq!(first.title, "Beginn der Rechtsfähigkeit");
        assert_eq!(
            first.text,
            "Die Rechtsfähigkeit des Menschen beginnt mit der Vollendung der Geburt."
        );
    }

    #[test]
    fn parse_multi_paragraph_norm() {
        let law = parse_law_xml(SAMPLE);
        let kauf = law
            .norms
            .iter()
            .find(|norm| norm.citation == "§ 433")
            .expect("§ 433");

        assert!(kauf.title.contains("Kaufvertrag"));
        assert!(kauf.text.starts_with("(1) Durch den Kaufvertrag"));
        assert!(kauf.text.contains("(2) Der Käufer ist verpflichtet"));
    }

    #[test]
    fn parse_gg_articles_without_title() {
        let law = parse_law_xml(GG_SAMPLE);

        assert_eq!(law.abbreviation, "GG");
        assert_eq!(law.norms[0].citation, "I.");
        assert_eq!(law.norms[0].title, "Die Grundrechte");
        let art1 = law
            .norms
            .iter()
            .find(|norm| norm.citation == "Art 1")
            .expect("Art 1");
        assert_eq!(art1.title, "");
        assert!(art1.text.contains("Würde des Menschen ist unantastbar"));
    }

    #[test]
    fn numbered_list_items_keep_markers() {
        let law = parse_law_xml(GG_SAMPLE);
        let art74 = law
            .norms
            .iter()
            .find(|norm| norm.citation == "Art 74")
            .expect("Art 74");

        assert!(art74.text.contains("folgende Gebiete:"));
        assert!(art74.text.contains("1. das bürgerliche Recht;"));
        assert!(art74.text.contains("2. das Personenstandswesen;"));
        assert!(art74
            .text
            .contains("19a. die wirtschaftliche Sicherung der Krankenhäuser;"));
        assert!(art74.text.contains("(2) Durch Bundesgesetz"));
    }
}
