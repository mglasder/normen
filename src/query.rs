use crate::catalog::{filter_from, resolve_in, sort_by_shortcut, LawRef};
use crate::cli::{CliFlags, Command, LawSort, QuerySpec};
use crate::models::{CitationKey, Law};
use crate::search::{lookup_norm, search_norms};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryFail {
    pub message: String,
    pub exit: u8,
}

#[derive(Serialize)]
struct LawRow {
    shortcut: String,
    slug: String,
    title: String,
}

#[derive(Serialize)]
struct LawsOut {
    query: String,
    laws: Vec<LawRow>,
}

#[derive(Serialize)]
struct OutlineNorm {
    citation: String,
    title: String,
}

#[derive(Serialize)]
struct OutlineOut {
    law: String,
    norms: Vec<OutlineNorm>,
}

#[derive(Serialize)]
struct GetOut {
    law: String,
    citation: String,
    title: String,
    text: String,
}

#[derive(Serialize)]
struct SearchHitOut {
    citation: String,
    title: String,
    preview: String,
    score: f64,
}

#[derive(Serialize)]
struct SearchOut {
    law: String,
    query: String,
    limit: Option<usize>,
    total: usize,
    hits: Vec<SearchHitOut>,
}

pub fn run_print(
    command: &Command,
    flags: &CliFlags,
    core: &[LawRef],
    load: impl Fn(&LawRef, bool) -> Result<Law, String>,
) -> Result<String, QueryFail> {
    match command {
        Command::Laws { filter, sort } => {
            let query = filter.clone().unwrap_or_default();
            let mut laws: Vec<LawRef> = filter_from(&query, core)
                .into_iter()
                .cloned()
                .collect();
            match sort {
                LawSort::Priority => {}
                LawSort::Alpha => sort_by_shortcut(&mut laws, false),
                LawSort::Rev => sort_by_shortcut(&mut laws, true),
            }
            let laws = laws
                .into_iter()
                .map(|law| LawRow {
                    shortcut: law.shortcut,
                    slug: law.slug,
                    title: law.title,
                })
                .collect();
            let out = LawsOut { query, laws };
            serde_json::to_string_pretty(&out).map_err(|e| QueryFail {
                message: e.to_string(),
                exit: 1,
            })
        }
        Command::Query { law, spec } => {
            let Some(law_ref) = resolve_in(law, core) else {
                return Err(QueryFail {
                    message: format!("unknown law: {law}"),
                    exit: 2,
                });
            };
            match spec {
                QuerySpec::Outline => {
                    let loaded = load(law_ref, flags.refresh).map_err(|message| QueryFail {
                        message,
                        exit: 1,
                    })?;
                    let out = OutlineOut {
                        law: law_ref.shortcut.clone(),
                        norms: loaded
                            .norms
                            .into_iter()
                            .map(|norm| OutlineNorm {
                                citation: norm.citation,
                                title: norm.title,
                            })
                            .collect(),
                    };
                    serde_json::to_string_pretty(&out).map_err(|e| QueryFail {
                        message: e.to_string(),
                        exit: 1,
                    })
                }
                QuerySpec::Get(raw) => {
                    if CitationKey::parse_query(raw).is_none() {
                        return Err(QueryFail {
                            message: format!("not a citation: {raw}"),
                            exit: 2,
                        });
                    }
                    let loaded = load(law_ref, flags.refresh).map_err(|message| QueryFail {
                        message,
                        exit: 1,
                    })?;
                    let Some(norm) = lookup_norm(&loaded, raw) else {
                        return Err(QueryFail {
                            message: format!("unknown citation: {raw}"),
                            exit: 2,
                        });
                    };
                    let out = GetOut {
                        law: law_ref.shortcut.clone(),
                        citation: norm.citation.clone(),
                        title: norm.title.clone(),
                        text: norm.text.clone(),
                    };
                    serde_json::to_string_pretty(&out).map_err(|e| QueryFail {
                        message: e.to_string(),
                        exit: 1,
                    })
                }
                QuerySpec::Search(needle) => {
                    let loaded = load(law_ref, flags.refresh).map_err(|message| QueryFail {
                        message,
                        exit: 1,
                    })?;
                    let hits_all = search_norms(&loaded, needle);
                    let total = hits_all.len();
                    let (hits, limit) = if flags.all {
                        (hits_all, None)
                    } else {
                        let cap = flags.limit.unwrap_or(10) as usize;
                        (
                            hits_all.into_iter().take(cap).collect(),
                            Some(cap),
                        )
                    };
                    let out = SearchOut {
                        law: law_ref.shortcut.clone(),
                        query: needle.clone(),
                        limit,
                        total,
                        hits: hits
                            .into_iter()
                            .map(|hit| SearchHitOut {
                                citation: hit.norm.citation,
                                title: hit.norm.title,
                                preview: hit.preview,
                                score: hit.score,
                            })
                            .collect(),
                    };
                    serde_json::to_string_pretty(&out).map_err(|e| QueryFail {
                        message: e.to_string(),
                        exit: 1,
                    })
                }
            }
        }
        _ => Err(QueryFail {
            message: "not a print command".into(),
            exit: 1,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::parse_args;
    use crate::catalog::{default_core, LawRef};
    use crate::models::{CitationKey, Law, Norm};
    use crate::parser::parse_law_xml;

    fn many_vertrag() -> Law {
        Law {
            abbreviation: "BGB".into(),
            title: "BGB".into(),
            norms: (1..=15)
                .map(|n| Norm {
                    citation: format!("§ {n}"),
                    title: format!("Norm {n} Vertrag"),
                    text: "Vertrag text".into(),
                    keys: vec![CitationKey::new(n)],
                })
                .collect(),
        }
    }

    const SAMPLE: &[u8] = include_bytes!("../tests/fixtures/sample.xml");

    fn unused_load(_: &LawRef, _: bool) -> Result<Law, String> {
        Err("load should not run for laws".into())
    }

    fn sample_load(law_ref: &LawRef, _: bool) -> Result<Law, String> {
        assert_eq!(law_ref.shortcut, "BGB");
        Ok(parse_law_xml(SAMPLE))
    }

    #[test]
    fn laws_unfiltered_includes_bgb() {
        let (cmd, flags) = parse_args(["normen", "laws"]).unwrap();
        let raw = run_print(&cmd, &flags, &default_core(), unused_load).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["query"], "");
        let laws = v["laws"].as_array().unwrap();
        assert!(laws.iter().any(|row| {
            row["shortcut"] == "BGB"
                && row["slug"] == "bgb"
                && row["title"] == "Bürgerliches Gesetzbuch"
        }));
        assert!(!raw.contains("\"kind\""));
    }

    #[test]
    fn laws_filter_buerger_is_bgb_and_egbgb() {
        let (cmd, flags) = parse_args(["normen", "laws", "bürger"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, &default_core(), unused_load).unwrap()).unwrap();
        assert_eq!(v["query"], "bürger");
        let shortcuts: Vec<_> = v["laws"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["shortcut"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(shortcuts, vec!["BGB", "EGBGB"]);
    }

    #[test]
    fn laws_empty_filter_is_empty_array_ok() {
        let (cmd, flags) = parse_args(["normen", "laws", "xyzzy"]).unwrap();
        let raw = run_print(&cmd, &flags, &default_core(), unused_load).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["query"], "xyzzy");
        assert_eq!(v["laws"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn unknown_law_is_exit_two_and_does_not_load() {
        let (cmd, flags) = parse_args(["normen", "xyzzy"]).unwrap();
        let err = run_print(&cmd, &flags, &default_core(), |_, _| panic!("must not load")).unwrap_err();
        assert_eq!(err.exit, 2);
        assert_eq!(err.message, "unknown law: xyzzy");
    }

    #[test]
    fn outline_lists_sample_norms() {
        let (cmd, flags) = parse_args(["normen", "BGB"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, &default_core(), sample_load).unwrap()).unwrap();
        assert_eq!(v["law"], "BGB");
        let citations: Vec<_> = v["norms"].as_array().unwrap().iter().map(|n| n["citation"].as_str().unwrap().to_string()).collect();
        assert!(citations.contains(&"§ 433".to_string()));
        let four33 = v["norms"].as_array().unwrap().iter().find(|n| n["citation"] == "§ 433").unwrap();
        assert_eq!(four33["title"], "Vertragstypische Pflichten beim Kaufvertrag");
        assert!(four33.get("text").is_none());
    }

    #[test]
    fn outline_passes_refresh_to_load() {
        let (cmd, flags) = parse_args(["normen", "--refresh", "BGB"]).unwrap();
        let err = run_print(&cmd, &flags, &default_core(), |_, refresh| {
            assert!(refresh);
            Err("saw refresh".into())
        }).unwrap_err();
        assert_eq!(err.exit, 1);
        assert_eq!(err.message, "saw refresh");
    }

    #[test]
    fn get_433_returns_norm_body() {
        let (cmd, flags) = parse_args(["normen", "bgb", "433"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, &default_core(), sample_load).unwrap()).unwrap();
        assert_eq!(v["law"], "BGB");
        assert_eq!(v["citation"], "§ 433");
        assert_eq!(v["title"], "Vertragstypische Pflichten beim Kaufvertrag");
        assert!(v["text"].as_str().unwrap().contains("Kaufvertrag"));
        assert!(v.get("keys").is_none());
    }

    #[test]
    fn get_unknown_number_is_exit_two() {
        let (cmd, flags) = parse_args(["normen", "BGB", "99999"]).unwrap();
        let err = run_print(&cmd, &flags, &default_core(), sample_load).unwrap_err();
        assert_eq!(err.exit, 2);
        assert_eq!(err.message, "unknown citation: 99999");
    }

    #[test]
    fn get_bare_word_is_not_a_citation() {
        let (cmd, flags) = parse_args(["normen", "BGB", "Kaufvertrag"]).unwrap();
        let err = run_print(&cmd, &flags, &default_core(), sample_load).unwrap_err();
        assert_eq!(err.exit, 2);
        assert_eq!(err.message, "not a citation: Kaufvertrag");
    }

    #[test]
    fn get_ignores_limit_flag() {
        let (cmd, flags) = parse_args(["normen", "--limit", "1", "BGB", "433"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, &default_core(), sample_load).unwrap()).unwrap();
        assert_eq!(v["citation"], "§ 433");
        assert!(v.get("hits").is_none());
    }

    #[test]
    fn search_default_limit_is_ten() {
        let (cmd, flags) = parse_args(["normen", "BGB", "/Vertrag"]).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &run_print(&cmd, &flags, &default_core(), |_, _| Ok(many_vertrag())).unwrap(),
        )
        .unwrap();
        assert_eq!(v["law"], "BGB");
        assert_eq!(v["query"], "Vertrag");
        assert_eq!(v["limit"], 10);
        assert_eq!(v["total"], 15);
        assert_eq!(v["hits"].as_array().unwrap().len(), 10);
        let hit = &v["hits"][0];
        assert!(hit.get("text").is_none());
        assert!(hit.get("citation").is_some());
        assert!(hit.get("title").is_some());
        assert!(hit.get("preview").is_some());
        assert!(hit.get("score").is_some());
    }

    #[test]
    fn search_limit_flag_caps_hits() {
        let (cmd, flags) = parse_args(["normen", "--limit", "3", "BGB", "/Vertrag"]).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &run_print(&cmd, &flags, &default_core(), |_, _| Ok(many_vertrag())).unwrap(),
        )
        .unwrap();
        assert_eq!(v["limit"], 3);
        assert_eq!(v["total"], 15);
        assert_eq!(v["hits"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn search_all_is_uncapped_null_limit() {
        let (cmd, flags) = parse_args(["normen", "--all", "BGB", "/Vertrag"]).unwrap();
        let v: serde_json::Value = serde_json::from_str(
            &run_print(&cmd, &flags, &default_core(), |_, _| Ok(many_vertrag())).unwrap(),
        )
        .unwrap();
        assert!(v["limit"].is_null());
        assert_eq!(v["total"], 15);
        assert_eq!(v["hits"].as_array().unwrap().len(), 15);
    }

    #[test]
    fn search_empty_hits_is_ok_document() {
        let (cmd, flags) = parse_args(["normen", "BGB", "/xyzzy"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, &default_core(), sample_load).unwrap()).unwrap();
        assert_eq!(v["total"], 0);
        assert_eq!(v["limit"], 10);
        assert_eq!(v["hits"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn search_uses_sample_kauf_preview() {
        let (cmd, flags) = parse_args(["normen", "BGB", "/Kaufvertrag"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, &default_core(), sample_load).unwrap()).unwrap();
        assert_eq!(v["hits"][0]["citation"], "§ 433");
        assert_eq!(v["hits"][0]["title"], "Vertragstypische Pflichten beim Kaufvertrag");
        assert!(v["hits"][0]["preview"].as_str().unwrap().to_lowercase().contains("kauf"));
    }

    #[test]
    fn print_path_has_no_lade_side_channel() {
        // run_print is the print product; it must not write to stdout/stderr.
        let (cmd, flags) = parse_args(["normen", "BGB", "433"]).unwrap();
        let json = run_print(&cmd, &flags, &default_core(), sample_load).unwrap();
        assert!(serde_json::from_str::<serde_json::Value>(&json).is_ok());
        assert!(!json.contains("Lade"));
    }

    #[test]
    fn query_fail_exit_codes_are_one_or_two() {
        let unknown = run_print(
            &parse_args(["normen", "nope"]).unwrap().0,
            &parse_args(["normen", "nope"]).unwrap().1,
            &default_core(),
            unused_load,
        )
        .unwrap_err();
        assert_eq!(unknown.exit, 2);
    }

    #[test]
    fn laws_sort_alpha_and_rev_by_shortcut() {
        let (cmd, flags) = parse_args(["normen", "laws", "--sort", "alpha"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, &default_core(), unused_load).unwrap())
                .unwrap();
        let shortcuts: Vec<_> = v["laws"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["shortcut"].as_str().unwrap().to_string())
            .collect();
        let mut expected = shortcuts.clone();
        expected.sort_by_key(|s| s.to_lowercase());
        assert_eq!(shortcuts, expected);
        let (cmd, flags) = parse_args(["normen", "laws", "--sort", "rev"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, &default_core(), unused_load).unwrap())
                .unwrap();
        let rev: Vec<_> = v["laws"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["shortcut"].as_str().unwrap().to_string())
            .collect();
        let mut expected_rev = expected;
        expected_rev.reverse();
        assert_eq!(rev, expected_rev);
    }

    #[test]
    fn laws_unknown_to_core_is_not_listed() {
        let core = vec![LawRef::new("BGB", "bgb", "Bürgerliches Gesetzbuch", &[])];
        let (cmd, flags) = parse_args(["normen", "laws"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, &core, unused_load).unwrap()).unwrap();
        let shortcuts: Vec<_> = v["laws"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["shortcut"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(shortcuts, vec!["BGB"]);
    }

    #[test]
    fn query_outside_core_is_unknown() {
        let core = vec![LawRef::new("BGB", "bgb", "Bürgerliches Gesetzbuch", &[])];
        let (cmd, flags) = parse_args(["normen", "StVG"]).unwrap();
        let err = run_print(&cmd, &flags, &core, |_, _| panic!("must not load")).unwrap_err();
        assert_eq!(err.exit, 2);
        assert_eq!(err.message, "unknown law: StVG");
    }
}
