use crate::catalog::{filter_laws, LawRef};
use crate::cli::{CliFlags, Command};
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

pub fn run_print(
    command: &Command,
    _flags: &CliFlags,
    _load: impl Fn(&LawRef, bool) -> Result<Law, String>,
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
        Command::Query { .. } => Err(QueryFail {
            message: "not implemented".into(),
            exit: 1,
        }),
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

    fn unused_load(_: &LawRef, _: bool) -> Result<Law, String> {
        Err("load should not run for laws".into())
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
}
