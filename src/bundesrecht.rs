use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog::{filter_from, LawRef};
use crate::fetch::{SOURCE_BASE, USER_AGENT};

const LETTERS: &[char] = &[
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedLaw {
    shortcut: String,
    slug: String,
    title: String,
}

pub fn cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join("bundesrecht.json")
}

pub fn load_cache(cache_dir: &Path) -> Vec<LawRef> {
    let Ok(text) = fs::read_to_string(cache_path(cache_dir)) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<CachedLaw>>(&text)
        .unwrap_or_default()
        .into_iter()
        .map(|row| LawRef::new(row.shortcut, row.slug, row.title, &[]))
        .collect()
}

pub fn save_cache(cache_dir: &Path, laws: &[LawRef]) -> Result<(), String> {
    fs::create_dir_all(cache_dir).map_err(|err| format!("cache: {err}"))?;
    let rows: Vec<CachedLaw> = laws
        .iter()
        .map(|law| CachedLaw {
            shortcut: law.shortcut.clone(),
            slug: law.slug.clone(),
            title: law.title.clone(),
        })
        .collect();
    let text = serde_json::to_string_pretty(&rows).map_err(|err| err.to_string())?;
    fs::write(cache_path(cache_dir), text).map_err(|err| format!("cache write: {err}"))
}

pub fn teilliste_url(letter: char) -> String {
    format!("{SOURCE_BASE}/Teilliste_{}.html", letter.to_ascii_uppercase())
}

pub fn parse_teilliste(html: &str) -> Vec<LawRef> {
    let mut laws = Vec::new();
    let mut rest = html;
    while let Some(href_at) = rest.find("href=") {
        let after_href = &rest[href_at + 5..];
        let quote = after_href.chars().next();
        let (quoted, after_quote) = match quote {
            Some(q @ ('"' | '\'')) => {
                let inner = &after_href[1..];
                match inner.find(q) {
                    Some(end) => (&inner[..end], &inner[end + 1..]),
                    None => {
                        rest = &after_href[1..];
                        continue;
                    }
                }
            }
            _ => {
                rest = after_href;
                continue;
            }
        };
        let Some(slug) = slug_from_href(quoted) else {
            rest = after_quote;
            continue;
        };
        let Some(gt) = after_quote.find('>') else {
            rest = after_quote;
            continue;
        };
        let inner = &after_quote[gt + 1..];
        let Some(close) = inner.find("</a>") else {
            rest = after_quote;
            continue;
        };
        let shortcut = strip_tags(&inner[..close]).trim().to_string();
        if shortcut.is_empty() || shortcut.eq_ignore_ascii_case("PDF") {
            rest = after_quote;
            continue;
        }
        let title = title_after_anchor(&inner[close + 4..]);
        if title.is_empty() {
            rest = after_quote;
            continue;
        }
        if !laws
            .iter()
            .any(|law: &LawRef| law.shortcut.eq_ignore_ascii_case(&shortcut))
        {
            laws.push(LawRef::new(shortcut, slug, title, &[]));
        }
        rest = after_quote;
    }
    laws
}

fn slug_from_href(href: &str) -> Option<String> {
    let href = href.trim();
    let href = href.strip_prefix("./").unwrap_or(href);
    let href = href.strip_prefix('/').unwrap_or(href);
    if href.contains("Teilliste_") || href.contains("aktuell") {
        return None;
    }
    let path = href.split('#').next().unwrap_or(href);
    let path = path.strip_suffix("/index.html").unwrap_or(path);
    let path = path.strip_suffix(".html").unwrap_or(path);
    let slug = path.rsplit('/').next().unwrap_or(path).trim();
    if slug.is_empty() || slug.contains('.') || slug == "index" {
        return None;
    }
    Some(slug.to_string())
}

fn strip_tags(raw: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    html_unescape(&out)
}

fn title_after_anchor(after_a: &str) -> String {
    let mut rest = after_a;
    if let Some(next_a) = rest.find("<a") {
        rest = &rest[..next_a];
    }
    if let Some(next_p) = rest.find("</p>") {
        rest = &rest[..next_p];
    }
    let cleaned = strip_tags(rest);
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    if words
        .last()
        .is_some_and(|word| word.eq_ignore_ascii_case("PDF"))
    {
        words.pop();
    }
    words.join(" ")
}

fn html_unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

pub fn fetch_index(
    download_page: impl Fn(char) -> Result<String, String>,
) -> Result<Vec<LawRef>, String> {
    let mut all = Vec::new();
    for &letter in LETTERS {
        let html = download_page(letter)?;
        for law in parse_teilliste(&html) {
            if !all
                .iter()
                .any(|existing: &LawRef| existing.shortcut.eq_ignore_ascii_case(&law.shortcut))
            {
                all.push(law);
            }
        }
    }
    Ok(all)
}

pub fn download_teilliste(letter: char) -> Result<String, String> {
    let url = teilliste_url(letter);
    let response = ureq::get(&url)
        .set("User-Agent", USER_AGENT)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|err| err.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|err| err.to_string())?;
    Ok(bytes.iter().map(|&b| char::from(b)).collect())
}

pub fn filter_letter<'a>(laws: &'a [LawRef], letter: char) -> Vec<&'a LawRef> {
    let want = letter.to_ascii_uppercase();
    laws.iter()
        .filter(|law| law.shortcut_letter() == Some(want))
        .collect()
}

pub fn filter_name<'a>(laws: &'a [LawRef], query: &str) -> Vec<&'a LawRef> {
    filter_from(query, laws)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
<p><a href="aag/index.html">AAG</a> Gesetz über den Ausgleich der Arbeitgeberaufwendungen PDF</p>
<p><a href="./stvg/index.html">StVG</a> Straßenverkehrsgesetz <a href="stvg/StVG.pdf">PDF</a></p>
<p><a href="bgb/index.html">BGB</a> Bürgerliches Gesetzbuch PDF</p>
<p><a href="Teilliste_B.html">B</a></p>
"#;

    const REAL_SHAPE: &str = r#"
<p><a href="./baazustv/index.html"><abbr title="Verordnung"> BAAZustV </abbr></a><br />
Verordnung zur Übertragung von Zuständigkeiten&nbsp;&nbsp;<a href="./baazustv/BAAZustV.pdf"><abbr>PDF</abbr></a> </p>
<p><a href="./bgb/index.html"><abbr title="Bürgerliches Gesetzbuch"> BGB </abbr></a><br />
Bürgerliches Gesetzbuch&nbsp;&nbsp;<a href="./bgb/BGB.pdf"><abbr>PDF</abbr></a> </p>
"#;

    #[test]
    fn teilliste_url_for_b() {
        assert_eq!(
            teilliste_url('b'),
            "https://www.gesetze-im-internet.de/Teilliste_B.html"
        );
    }

    #[test]
    fn parse_teilliste_reads_shortcut_slug_title() {
        let laws = parse_teilliste(SAMPLE);
        assert_eq!(laws.len(), 3);
        assert_eq!(laws[0].shortcut, "AAG");
        assert_eq!(laws[0].slug, "aag");
        assert!(laws[0].title.contains("Arbeitgeberaufwendungen"));
        assert_eq!(laws[1].shortcut, "StVG");
        assert_eq!(laws[1].slug, "stvg");
        assert_eq!(laws[1].title, "Straßenverkehrsgesetz");
        assert_eq!(laws[2].shortcut, "BGB");
    }

    #[test]
    fn parse_teilliste_abbr_and_br_title() {
        let laws = parse_teilliste(REAL_SHAPE);
        assert_eq!(laws.len(), 2);
        assert_eq!(laws[0].shortcut, "BAAZustV");
        assert_eq!(laws[0].slug, "baazustv");
        assert!(laws[0].title.contains("Zuständigkeiten"));
        assert_eq!(laws[1].shortcut, "BGB");
        assert_eq!(laws[1].slug, "bgb");
        assert_eq!(laws[1].title, "Bürgerliches Gesetzbuch");
    }

    #[test]
    fn filter_letter_uses_shortcut_first_char() {
        let laws = parse_teilliste(SAMPLE);
        let b = filter_letter(&laws, 'b');
        assert_eq!(
            b.iter().map(|law| law.shortcut.as_str()).collect::<Vec<_>>(),
            vec!["BGB"]
        );
        let s = filter_letter(&laws, 'S');
        assert_eq!(s[0].shortcut, "StVG");
    }

    #[test]
    fn filter_name_matches_title_substring() {
        let laws = parse_teilliste(SAMPLE);
        let hits = filter_name(&laws, "straße");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].shortcut, "StVG");
    }

    #[test]
    fn cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let laws = parse_teilliste(SAMPLE);
        save_cache(dir.path(), &laws).unwrap();
        let loaded = load_cache(dir.path());
        assert_eq!(loaded[1].shortcut, "StVG");
        assert_eq!(loaded[1].slug, "stvg");
    }

    #[test]
    fn fetch_index_walks_letters() {
        let html_a = r#"<a href="aag/index.html">AAG</a> Ausgleich PDF"#;
        let html_s = r#"<a href="stvg/index.html">StVG</a> Straßenverkehrsgesetz PDF"#;
        let laws = fetch_index(|letter| match letter {
            'A' => Ok(html_a.into()),
            'S' => Ok(html_s.into()),
            _ => Ok(String::new()),
        })
        .unwrap();
        assert!(laws.iter().any(|law| law.shortcut == "AAG"));
        assert!(laws.iter().any(|law| law.shortcut == "StVG"));
    }
}
