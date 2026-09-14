#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawRef {
    pub shortcut: String,
    pub slug: String,
    pub title: String,
    pub aliases: Vec<String>,
}

impl LawRef {
    pub fn new(
        shortcut: impl Into<String>,
        slug: impl Into<String>,
        title: impl Into<String>,
        aliases: &[&str],
    ) -> Self {
        Self {
            shortcut: shortcut.into(),
            slug: slug.into(),
            title: title.into(),
            aliases: aliases.iter().map(|alias| alias.to_string()).collect(),
        }
    }

    pub fn matches_key(&self, key: &str) -> bool {
        self.shortcut.eq_ignore_ascii_case(key)
            || self.slug.eq_ignore_ascii_case(key)
            || self
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(key))
    }

    pub fn contains_key(&self, key: &str) -> bool {
        std::iter::once(self.shortcut.as_str())
            .chain(std::iter::once(self.slug.as_str()))
            .chain(std::iter::once(self.title.as_str()))
            .chain(self.aliases.iter().map(String::as_str))
            .any(|part| part.to_lowercase().contains(key))
    }

    pub fn shortcut_letter(&self) -> Option<char> {
        self.shortcut.chars().next().map(|ch| ch.to_ascii_uppercase())
    }
}

pub fn default_core() -> Vec<LawRef> {
    vec![
        LawRef::new("BGB", "bgb", "Bürgerliches Gesetzbuch", &[]),
        LawRef::new(
            "GG",
            "gg",
            "Grundgesetz für die Bundesrepublik Deutschland",
            &[],
        ),
        LawRef::new("VwGO", "vwgo", "Verwaltungsgerichtsordnung", &[]),
        LawRef::new("VwVfG", "vwvfg", "Verwaltungsverfahrensgesetz", &["vwvwfg"]),
        LawRef::new(
            "BVerfGG",
            "bverfgg",
            "Gesetz über das Bundesverfassungsgericht",
            &[],
        ),
        LawRef::new(
            "GOBT",
            "btgo_2025",
            "Geschäftsordnung des Deutschen Bundestages",
            &["gobt", "btgo", "go-bt"],
        ),
        LawRef::new(
            "GOBR",
            "brgo_2025",
            "Geschäftsordnung des Bundesrates",
            &["gobr", "brgo", "go-br"],
        ),
        LawRef::new(
            "PartG",
            "partg",
            "Gesetz über die politischen Parteien",
            &["parteig", "parteiengesetz", "parteiegesetz"],
        ),
        LawRef::new(
            "VereinsG",
            "vereinsg",
            "Gesetz zur Regelung des öffentlichen Vereinsrechts",
            &["vereinsgesetz"],
        ),
        LawRef::new(
            "VersammlG",
            "versammlg",
            "Gesetz über Versammlungen und Aufzüge",
            &["versammlungsgesetz", "versammunglusgesetzt"],
        ),
        LawRef::new("BauGB", "bbaug", "Baugesetzbuch", &["bbaug", "bbau"]),
        LawRef::new("BauNVO", "baunvo", "Baunutzungsverordnung", &[]),
        LawRef::new(
            "VwZG",
            "vwzg_2005",
            "Verwaltungszustellungsgesetz",
            &["vwzg"],
        ),
        LawRef::new("VwVG", "vwvg", "Verwaltungs-Vollstreckungsgesetz", &[]),
        LawRef::new("StGB", "stgb", "Strafgesetzbuch", &[]),
        LawRef::new("ZPO", "zpo", "Zivilprozessordnung", &[]),
        LawRef::new("StPO", "stpo", "Strafprozeßordnung", &[]),
        LawRef::new("HGB", "hgb", "Handelsgesetzbuch", &[]),
        LawRef::new(
            "EGBGB",
            "bgbeg",
            "Einführungsgesetz zum Bürgerlichen Gesetzbuche",
            &["egbgb", "bgbeg"],
        ),
    ]
}

/// Resolve CORE from an optional conf `order` list.
///
/// `None` means the section is missing: use the shipped seed.
/// Unknown shortcuts are skipped and returned as warnings.
pub fn resolve_core(order: Option<&[String]>, index: &[LawRef]) -> (Vec<LawRef>, Vec<String>) {
    let Some(order) = order else {
        return (default_core(), Vec::new());
    };
    let seed = default_core();
    let mut core = Vec::new();
    let mut warnings = Vec::new();
    for raw in order {
        let key = raw.trim();
        if key.is_empty() {
            continue;
        }
        if let Some(law) = find_law(key, &seed).or_else(|| find_law(key, index)) {
            if core
                .iter()
                .any(|existing: &LawRef| existing.shortcut.eq_ignore_ascii_case(&law.shortcut))
            {
                continue;
            }
            core.push(law.clone());
        } else {
            warnings.push(key.to_string());
        }
    }
    (core, warnings)
}

pub fn resolve_in<'a>(query: &str, laws: &'a [LawRef]) -> Option<&'a LawRef> {
    let key = query.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    laws.iter().find(|law| law.matches_key(&key))
}

pub fn resolve_law(query: &str) -> Option<LawRef> {
    resolve_in(query, &default_core()).cloned()
}

pub fn filter_from<'a>(query: &str, laws: &'a [LawRef]) -> Vec<&'a LawRef> {
    let key = query.trim().to_lowercase();
    if key.is_empty() {
        return laws.iter().collect();
    }
    laws.iter().filter(|law| law.contains_key(&key)).collect()
}

pub fn filter_laws(query: &str) -> Vec<LawRef> {
    filter_from(query, &default_core())
        .into_iter()
        .cloned()
        .collect()
}

pub fn sort_by_shortcut(laws: &mut [LawRef], reverse: bool) {
    laws.sort_by(|a, b| {
        let left = a.shortcut.to_lowercase();
        let right = b.shortcut.to_lowercase();
        if reverse {
            right.cmp(&left)
        } else {
            left.cmp(&right)
        }
    });
}

pub fn apply_marks(core: &[LawRef], index: &[LawRef], marked: &[String]) -> Vec<LawRef> {
    let mut next = core.to_vec();
    for mark in marked {
        let on_core = next
            .iter()
            .any(|law| law.shortcut.eq_ignore_ascii_case(mark));
        if on_core {
            next.retain(|law| !law.shortcut.eq_ignore_ascii_case(mark));
            continue;
        }
        if let Some(law) = find_law(mark, index) {
            if !next
                .iter()
                .any(|existing| existing.shortcut.eq_ignore_ascii_case(&law.shortcut))
            {
                next.push(law.clone());
            }
        }
    }
    next
}

pub fn remove_from_core(core: &[LawRef], shortcut: &str) -> Vec<LawRef> {
    core.iter()
        .filter(|law| !law.shortcut.eq_ignore_ascii_case(shortcut))
        .cloned()
        .collect()
}

fn find_law<'a>(query: &str, laws: &'a [LawRef]) -> Option<&'a LawRef> {
    resolve_in(query, laws)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn resolve_law_bgb() {
        let law = resolve_law("bgb").expect("bgb should resolve");
        assert_eq!(law.shortcut, "BGB");
    }

    #[test]
    fn supported_shortcuts() {
        let shortcuts: HashSet<_> = default_core()
            .iter()
            .map(|law| law.shortcut.to_uppercase())
            .collect();
        let expected: HashSet<_> = [
            "BGB", "GG", "VWGO", "VWVFG", "BVERFGG", "GOBT", "GOBR", "PARTG", "VEREINSG",
            "VERSAMMLG", "BAUGB", "BAUNVO", "VWZG", "VWVG", "STGB", "ZPO", "STPO", "HGB", "EGBGB",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(shortcuts, expected);
    }

    #[test]
    fn resolve_law_is_case_insensitive() {
        assert_eq!(resolve_law("bgb").unwrap().shortcut, "BGB");
        assert_eq!(resolve_law("VwVfG").unwrap().slug, "vwvfg");
        assert!(resolve_law("vwgo")
            .unwrap()
            .title
            .starts_with("Verwaltungsgerichts"));
    }

    #[test]
    fn resolve_law_accepts_common_typo_alias() {
        assert_eq!(resolve_law("vwvwfg").unwrap().shortcut, "VwVfG");
    }

    #[test]
    fn resolve_law_accepts_new_shortcuts_and_slugs() {
        assert_eq!(resolve_law("bverfgg").unwrap().slug, "bverfgg");
        assert_eq!(resolve_law("gobt").unwrap().slug, "btgo_2025");
        assert_eq!(resolve_law("btgo").unwrap().shortcut, "GOBT");
        assert_eq!(resolve_law("gobr").unwrap().slug, "brgo_2025");
        assert!(resolve_law("partg")
            .unwrap()
            .title
            .starts_with("Gesetz über die politischen Parteien"));
        assert_eq!(resolve_law("parteiengesetz").unwrap().shortcut, "PartG");
        assert_eq!(resolve_law("vereinsg").unwrap().shortcut, "VereinsG");
        assert_eq!(
            resolve_law("versammlungsgesetz").unwrap().shortcut,
            "VersammlG"
        );
        assert_eq!(resolve_law("baugb").unwrap().slug, "bbaug");
        assert_eq!(resolve_law("baunvo").unwrap().shortcut, "BauNVO");
        assert_eq!(resolve_law("vwzg").unwrap().slug, "vwzg_2005");
        assert_eq!(resolve_law("vwvg").unwrap().shortcut, "VwVG");
        assert_eq!(resolve_law("stgb").unwrap().shortcut, "StGB");
        assert_eq!(resolve_law("zpo").unwrap().shortcut, "ZPO");
        assert_eq!(resolve_law("stpo").unwrap().shortcut, "StPO");
        assert_eq!(resolve_law("hgb").unwrap().shortcut, "HGB");
        assert_eq!(resolve_law("egbgb").unwrap().slug, "bgbeg");
        assert_eq!(resolve_law("bgbeg").unwrap().shortcut, "EGBGB");
    }

    #[test]
    fn resolve_unknown_law_returns_none() {
        assert!(resolve_law("xyzzy").is_none());
    }

    #[test]
    fn filter_laws_empty_query_returns_all() {
        let all = default_core();
        assert_eq!(filter_laws(""), all);
        assert_eq!(filter_laws("   "), all);
    }

    #[test]
    fn filter_laws_matches_shortcut_slug_title_and_alias() {
        let shortcuts = |query: &str| {
            filter_laws(query)
                .into_iter()
                .map(|law| law.shortcut)
                .collect::<Vec<_>>()
        };
        assert_eq!(shortcuts("bgb"), vec!["BGB", "EGBGB"]);
        assert_eq!(shortcuts("BÜRGER"), vec!["BGB", "EGBGB"]);
        assert_eq!(shortcuts("grund"), vec!["GG"]);
        assert_eq!(shortcuts("vwvwfg"), vec!["VwVfG"]);
        assert_eq!(shortcuts("vw"), vec!["VwGO", "VwVfG", "VwZG", "VwVG"]);
    }

    #[test]
    fn filter_laws_unknown_query_is_empty() {
        assert!(filter_laws("xyzzy").is_empty());
    }

    fn stvg() -> LawRef {
        LawRef::new(
            "StVG",
            "stvg",
            "Straßenverkehrsgesetz",
            &[],
        )
    }

    #[test]
    fn missing_order_uses_shipped_seed() {
        let (core, warnings) = resolve_core(None, &[]);
        assert_eq!(core, default_core());
        assert!(warnings.is_empty());
    }

    #[test]
    fn empty_order_is_empty_core() {
        let (core, warnings) = resolve_core(Some(&[]), &[]);
        assert!(core.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn order_resolves_seed_then_index() {
        let order = vec!["BGB".into(), "StVG".into()];
        let (core, warnings) = resolve_core(Some(&order), &[stvg()]);
        assert_eq!(
            core.iter().map(|law| law.shortcut.as_str()).collect::<Vec<_>>(),
            vec!["BGB", "StVG"]
        );
        assert_eq!(core[1].slug, "stvg");
        assert!(warnings.is_empty());
    }

    #[test]
    fn unknown_shortcut_is_skipped_with_warning() {
        let order = vec!["BGB".into(), "STVGG".into(), "GG".into()];
        let (core, warnings) = resolve_core(Some(&order), &[stvg()]);
        assert_eq!(
            core.iter().map(|law| law.shortcut.as_str()).collect::<Vec<_>>(),
            vec!["BGB", "GG"]
        );
        assert_eq!(warnings, vec!["STVGG"]);
    }

    #[test]
    fn marks_append_adds_and_remove_drops() {
        let core = default_core();
        let index = vec![stvg()];
        let marked = vec!["StVG".into(), "GG".into()];
        let next = apply_marks(&core, &index, &marked);
        assert!(next.iter().any(|law| law.shortcut == "StVG"));
        assert!(next.last().unwrap().shortcut == "StVG");
        assert!(!next.iter().any(|law| law.shortcut == "GG"));
    }

    #[test]
    fn remove_last_law_allows_empty_core() {
        let core = vec![LawRef::new("BGB", "bgb", "Bürgerliches Gesetzbuch", &[])];
        let next = remove_from_core(&core, "BGB");
        assert!(next.is_empty());
    }

    #[test]
    fn sort_by_shortcut_alpha_and_rev() {
        let mut laws = vec![
            LawRef::new("GG", "gg", "Grundgesetz", &[]),
            LawRef::new("BGB", "bgb", "BGB", &[]),
            LawRef::new("ZPO", "zpo", "ZPO", &[]),
        ];
        sort_by_shortcut(&mut laws, false);
        assert_eq!(
            laws.iter().map(|l| l.shortcut.as_str()).collect::<Vec<_>>(),
            vec!["BGB", "GG", "ZPO"]
        );
        sort_by_shortcut(&mut laws, true);
        assert_eq!(
            laws.iter().map(|l| l.shortcut.as_str()).collect::<Vec<_>>(),
            vec!["ZPO", "GG", "BGB"]
        );
    }

    #[test]
    fn seed_aliases_survive_core_resolution() {
        let order = vec!["VwVfG".into()];
        let (core, _) = resolve_core(Some(&order), &[]);
        assert_eq!(core[0].aliases, vec!["vwvwfg"]);
        assert!(resolve_in("vwvwfg", &core).is_some());
    }
}
