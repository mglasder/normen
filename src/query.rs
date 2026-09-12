use crate::catalog::{filter_laws, resolve_law, LawRef};
use crate::cli::{CliFlags, Command, QuerySpec};
use crate::models::Law;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryFail {
    pub message: String,
    pub exit: u8,
}

#[derive(Serialize)]
struct LawRow {
    shortcut: &'static str,
    slug: &'static str,
    title: &'static str,
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
    law: &'static str,
    norms: Vec<OutlineNorm>,
}

pub fn run_print(
    command: &Command,
    flags: &CliFlags,
    load: impl Fn(&LawRef, bool) -> Result<Law, String>,
) -> Result<String, QueryFail> {
    match command {
        Command::Laws { filter } => {
            let query = filter.clone().unwrap_or_default();
            let laws = filter_laws(&query)
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
            let Some(law_ref) = resolve_law(law) else {
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
                        law: law_ref.shortcut,
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
                _ => Err(QueryFail {
                    message: "not implemented".into(),
                    exit: 1,
                }),
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
    use crate::catalog::LawRef;
    use crate::models::Law;
    use crate::parser::parse_law_xml;

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
        let raw = run_print(&cmd, &flags, unused_load).unwrap();
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
            serde_json::from_str(&run_print(&cmd, &flags, unused_load).unwrap()).unwrap();
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
        let raw = run_print(&cmd, &flags, unused_load).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["query"], "xyzzy");
        assert_eq!(v["laws"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn unknown_law_is_exit_two_and_does_not_load() {
        let (cmd, flags) = parse_args(["normen", "xyzzy"]).unwrap();
        let err = run_print(&cmd, &flags, |_, _| panic!("must not load")).unwrap_err();
        assert_eq!(err.exit, 2);
        assert_eq!(err.message, "unknown law: xyzzy");
    }

    #[test]
    fn outline_lists_sample_norms() {
        let (cmd, flags) = parse_args(["normen", "BGB"]).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&run_print(&cmd, &flags, sample_load).unwrap()).unwrap();
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
        let err = run_print(&cmd, &flags, |_, refresh| {
            assert!(refresh);
            Err("saw refresh".into())
        }).unwrap_err();
        assert_eq!(err.exit, 1);
        assert_eq!(err.message, "saw refresh");
    }
}
