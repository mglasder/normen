#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LawRef {
    pub shortcut: &'static str,
    pub slug: &'static str,
    pub title: &'static str,
    pub aliases: &'static [&'static str],
}

pub static LAWS: &[LawRef] = &[
    LawRef {
        shortcut: "BGB",
        slug: "bgb",
        title: "Bürgerliches Gesetzbuch",
        aliases: &[],
    },
    LawRef {
        shortcut: "GG",
        slug: "gg",
        title: "Grundgesetz für die Bundesrepublik Deutschland",
        aliases: &[],
    },
    LawRef {
        shortcut: "VwGO",
        slug: "vwgo",
        title: "Verwaltungsgerichtsordnung",
        aliases: &[],
    },
    LawRef {
        shortcut: "VwVfG",
        slug: "vwvfg",
        title: "Verwaltungsverfahrensgesetz",
        aliases: &["vwvwfg"],
    },
    LawRef {
        shortcut: "BVerfGG",
        slug: "bverfgg",
        title: "Gesetz über das Bundesverfassungsgericht",
        aliases: &[],
    },
    LawRef {
        shortcut: "GOBT",
        slug: "btgo_2025",
        title: "Geschäftsordnung des Deutschen Bundestages",
        aliases: &["gobt", "btgo", "go-bt"],
    },
    LawRef {
        shortcut: "GOBR",
        slug: "brgo_2025",
        title: "Geschäftsordnung des Bundesrates",
        aliases: &["gobr", "brgo", "go-br"],
    },
    LawRef {
        shortcut: "PartG",
        slug: "partg",
        title: "Gesetz über die politischen Parteien",
        aliases: &["parteig", "parteiengesetz", "parteiegesetz"],
    },
    LawRef {
        shortcut: "VereinsG",
        slug: "vereinsg",
        title: "Gesetz zur Regelung des öffentlichen Vereinsrechts",
        aliases: &["vereinsgesetz"],
    },
    LawRef {
        shortcut: "VersammlG",
        slug: "versammlg",
        title: "Gesetz über Versammlungen und Aufzüge",
        aliases: &["versammlungsgesetz", "versammunglusgesetzt"],
    },
    LawRef {
        shortcut: "BauGB",
        slug: "bbaug",
        title: "Baugesetzbuch",
        aliases: &["bbaug", "bbau"],
    },
    LawRef {
        shortcut: "BauNVO",
        slug: "baunvo",
        title: "Baunutzungsverordnung",
        aliases: &[],
    },
    LawRef {
        shortcut: "VwZG",
        slug: "vwzg_2005",
        title: "Verwaltungszustellungsgesetz",
        aliases: &["vwzg"],
    },
    LawRef {
        shortcut: "VwVG",
        slug: "vwvg",
        title: "Verwaltungs-Vollstreckungsgesetz",
        aliases: &[],
    },
    LawRef {
        shortcut: "StGB",
        slug: "stgb",
        title: "Strafgesetzbuch",
        aliases: &[],
    },
    LawRef {
        shortcut: "ZPO",
        slug: "zpo",
        title: "Zivilprozessordnung",
        aliases: &[],
    },
    LawRef {
        shortcut: "StPO",
        slug: "stpo",
        title: "Strafprozeßordnung",
        aliases: &[],
    },
    LawRef {
        shortcut: "HGB",
        slug: "hgb",
        title: "Handelsgesetzbuch",
        aliases: &[],
    },
    LawRef {
        shortcut: "EGBGB",
        slug: "bgbeg",
        title: "Einführungsgesetz zum Bürgerlichen Gesetzbuche",
        aliases: &["egbgb", "bgbeg"],
    },
];

pub fn resolve_law(query: &str) -> Option<&'static LawRef> {
    let key = query.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    LAWS.iter().find(|law| {
        law.shortcut.to_lowercase() == key
            || law.slug.to_lowercase() == key
            || law.aliases.iter().any(|alias| *alias == key)
    })
}

pub fn filter_laws(query: &str) -> Vec<&'static LawRef> {
    filter_laws_from(query, LAWS)
}

pub fn filter_laws_from<'a>(query: &str, laws: &'a [LawRef]) -> Vec<&'a LawRef> {
    let key = query.trim().to_lowercase();
    if key.is_empty() {
        return laws.iter().collect();
    }
    laws.iter()
        .filter(|law| {
            std::iter::once(law.shortcut)
                .chain(std::iter::once(law.slug))
                .chain(std::iter::once(law.title))
                .chain(law.aliases.iter().copied())
                .any(|part| part.to_lowercase().contains(&key))
        })
        .collect()
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
        let shortcuts: HashSet<_> = LAWS.iter().map(|law| law.shortcut.to_uppercase()).collect();
        let expected: HashSet<_> = [
            "BGB",
            "GG",
            "VWGO",
            "VWVFG",
            "BVERFGG",
            "GOBT",
            "GOBR",
            "PARTG",
            "VEREINSG",
            "VERSAMMLG",
            "BAUGB",
            "BAUNVO",
            "VWZG",
            "VWVG",
            "STGB",
            "ZPO",
            "STPO",
            "HGB",
            "EGBGB",
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
        assert_eq!(filter_laws(""), LAWS.iter().collect::<Vec<_>>());
        assert_eq!(filter_laws("   "), LAWS.iter().collect::<Vec<_>>());
    }

    #[test]
    fn filter_laws_matches_shortcut_slug_title_and_alias() {
        assert_eq!(
            filter_laws("bgb")
                .iter()
                .map(|law| law.shortcut)
                .collect::<Vec<_>>(),
            vec!["BGB", "EGBGB"]
        );
        assert_eq!(
            filter_laws("BÜRGER")
                .iter()
                .map(|law| law.shortcut)
                .collect::<Vec<_>>(),
            vec!["BGB", "EGBGB"]
        );
        assert_eq!(
            filter_laws("grund")
                .iter()
                .map(|law| law.shortcut)
                .collect::<Vec<_>>(),
            vec!["GG"]
        );
        assert_eq!(
            filter_laws("vwvwfg")
                .iter()
                .map(|law| law.shortcut)
                .collect::<Vec<_>>(),
            vec!["VwVfG"]
        );
        assert_eq!(
            filter_laws("vw")
                .iter()
                .map(|law| law.shortcut)
                .collect::<Vec<_>>(),
            vec!["VwGO", "VwVfG", "VwZG", "VwVG"]
        );
    }

    #[test]
    fn filter_laws_unknown_query_is_empty() {
        assert!(filter_laws("xyzzy").is_empty());
    }
}
