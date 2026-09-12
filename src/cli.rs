use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::{Parser, Subcommand};

use crate::catalog::resolve_law;
use crate::session::WorkspaceStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuerySpec {
    Outline,
    Get(String),
    Search(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Open,
    Query { law: String, spec: QuerySpec },
    Laws { filter: Option<String> },
    Attach { id: Option<u32> },
    List,
    Remove { id: Option<u32>, all: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliFlags {
    pub refresh: bool,
    pub limit: Option<u32>,
    pub all: bool,
}

#[derive(Parser)]
#[command(
    name = "normen",
    about = "Deutsche Gesetze lesen und durchsuchen (gesetze-im-internet.de).",
    args_conflicts_with_subcommands = true,
    disable_help_subcommand = true
)]
struct Cli {
    /// Gesetzestexte neu von gesetze-im-internet.de laden
    #[arg(long)]
    refresh: bool,
    #[arg(long)]
    limit: Option<u32>,
    #[arg(long, action = clap::ArgAction::SetTrue)]
    all: bool,
    #[command(subcommand)]
    command: Option<SubCommand>,
    /// Kürzel, z.B. BGB, GG, StGB, VwGO
    law: Option<String>,
    /// Normnummer (433, 31a) oder /Volltextsuche
    norm: Option<String>,
}

#[derive(Subcommand)]
enum SubCommand {
    /// Resume a persisted workspace
    Attach {
        id: Option<u32>,
    },
    /// List persisted workspaces
    List,
    /// List or filter available laws
    Laws {
        filter: Option<String>,
    },
    /// Delete persisted workspaces
    Rm {
        id: Option<u32>,
        #[arg(long)]
        all: bool,
    },
}

pub fn parse_args<I, T>(args: I) -> Result<(Command, CliFlags), String>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(err) => {
            if matches!(
                err.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                err.exit();
            }
            return Err(err.to_string());
        }
    };
    if cli.limit == Some(0) {
        return Err("--limit must be >= 1 (use --all for no cap)".into());
    }
    if cli.all && cli.limit.is_some() {
        return Err("use --all or --limit, not both".into());
    }
    let command = match cli.command {
        Some(SubCommand::Attach { id }) => Command::Attach { id },
        Some(SubCommand::List) => Command::List,
        Some(SubCommand::Laws { filter }) => Command::Laws { filter },
        Some(SubCommand::Rm { id, all }) => {
            if !all && id.is_none() {
                return Err("rm requires an id or --all".into());
            }
            if all && id.is_some() {
                return Err("rm: use an id or --all, not both".into());
            }
            Command::Remove { id, all }
        }
        None => match cli.law {
            None => Command::Open,
            Some(law) => Command::Query {
                law,
                spec: classify_norm(cli.norm),
            },
        },
    };
    Ok((
        command,
        CliFlags {
            refresh: cli.refresh,
            limit: cli.limit,
            all: cli.all,
        },
    ))
}

fn classify_norm(norm: Option<String>) -> QuerySpec {
    match norm {
        None => QuerySpec::Outline,
        Some(raw) if raw.starts_with('/') => {
            QuerySpec::Search(raw.strip_prefix('/').unwrap_or(&raw).to_string())
        }
        Some(raw) => QuerySpec::Get(raw),
    }
}

pub fn format_list(store: &WorkspaceStore) -> String {
    let mut lines = vec!["ID  TABS".to_string()];
    for workspace in store.list() {
        let mut tabs: Vec<String> = workspace
            .tabs
            .iter()
            .enumerate()
            .map(|(index, tab)| {
                let name = resolve_law(&tab.slug)
                    .map(|law| law.shortcut.to_string())
                    .unwrap_or_else(|| tab.slug.to_uppercase());
                if workspace.active == index as i32 {
                    format!("{name}*")
                } else {
                    name
                }
            })
            .collect();
        if workspace.active < 0 {
            tabs.insert(0, "MENU*".into());
        }
        lines.push(format!("{:<3} {}", workspace.id, tabs.join(" ")));
    }
    lines.join("\n")
}

pub fn apply_cli(cmd: &Command, store: &mut WorkspaceStore) -> Result<String, String> {
    match cmd {
        Command::List => Ok(format_list(store)),
        Command::Remove { all: true, .. } => {
            store.remove_all();
            Ok(String::new())
        }
        Command::Remove {
            id: Some(id),
            all: false,
        } => {
            if store.remove(*id) {
                Ok(String::new())
            } else {
                Err(format!("no persisted session {id}"))
            }
        }
        Command::Remove { id: None, all: false } => {
            Err("rm requires an id or --all".into())
        }
        Command::Open | Command::Attach { .. } | Command::Query { .. } | Command::Laws { .. } => {
            Err("not a store command".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_law_only_is_query_outline() {
        let (cmd, flags) = parse_args(["normen", "BGB"]).unwrap();
        assert_eq!(
            cmd,
            Command::Query {
                law: "BGB".into(),
                spec: QuerySpec::Outline
            }
        );
        assert_eq!(
            flags,
            CliFlags {
                refresh: false,
                limit: None,
                all: false
            }
        );
    }

    #[test]
    fn parse_law_and_citation_is_query_get() {
        let (cmd, _) = parse_args(["normen", "BGB", "433"]).unwrap();
        assert_eq!(
            cmd,
            Command::Query {
                law: "BGB".into(),
                spec: QuerySpec::Get("433".into())
            }
        );
    }

    #[test]
    fn parse_slash_is_query_search() {
        let (cmd, _) = parse_args(["normen", "BGB", "/kauf"]).unwrap();
        assert_eq!(
            cmd,
            Command::Query {
                law: "BGB".into(),
                spec: QuerySpec::Search("kauf".into())
            }
        );
    }

    #[test]
    fn parse_bare_word_second_arg_is_still_get() {
        let (cmd, _) = parse_args(["normen", "BGB", "Kaufvertrag"]).unwrap();
        assert_eq!(
            cmd,
            Command::Query {
                law: "BGB".into(),
                spec: QuerySpec::Get("Kaufvertrag".into())
            }
        );
    }

    #[test]
    fn parse_laws_and_filter() {
        assert_eq!(
            parse_args(["normen", "laws"]).unwrap().0,
            Command::Laws { filter: None }
        );
        assert_eq!(
            parse_args(["normen", "laws", "bürger"]).unwrap().0,
            Command::Laws {
                filter: Some("bürger".into())
            }
        );
    }

    #[test]
    fn parse_limit_and_all_flags() {
        let (_, flags) = parse_args(["normen", "--limit", "20", "BGB", "/kauf"]).unwrap();
        assert_eq!(flags.limit, Some(20));
        let (_, flags) = parse_args(["normen", "BGB", "/kauf", "--all"]).unwrap();
        assert!(flags.all);
    }

    #[test]
    fn parse_limit_zero_is_usage_error() {
        let err = parse_args(["normen", "--limit", "0", "BGB", "/kauf"]).unwrap_err();
        assert_eq!(err, "--limit must be >= 1 (use --all for no cap)");
    }

    #[test]
    fn parse_all_with_limit_is_usage_error() {
        let err = parse_args(["normen", "--all", "--limit", "5", "BGB", "/kauf"]).unwrap_err();
        assert_eq!(err, "use --all or --limit, not both");
    }

    #[test]
    fn parse_bare_normen_is_open_unit() {
        let (cmd, flags) = parse_args(["normen"]).unwrap();
        assert_eq!(cmd, Command::Open);
        assert!(!flags.refresh);
    }

    #[test]
    fn parse_refresh_with_law_is_query() {
        let (cmd, flags) = parse_args(["normen", "--refresh", "bgb"]).unwrap();
        assert_eq!(
            cmd,
            Command::Query {
                law: "bgb".into(),
                spec: QuerySpec::Outline
            }
        );
        assert!(flags.refresh);
    }

    #[test]
    fn parse_attach_last_and_id() {
        assert_eq!(
            parse_args(["normen", "attach"]).unwrap().0,
            Command::Attach { id: None }
        );
        assert_eq!(
            parse_args(["normen", "attach", "3"]).unwrap().0,
            Command::Attach { id: Some(3) }
        );
    }

    #[test]
    fn parse_list_and_rm() {
        assert_eq!(parse_args(["normen", "list"]).unwrap().0, Command::List);
        assert_eq!(
            parse_args(["normen", "rm", "--all"]).unwrap().0,
            Command::Remove {
                id: None,
                all: true
            }
        );
        assert_eq!(
            parse_args(["normen", "rm", "2"]).unwrap().0,
            Command::Remove {
                id: Some(2),
                all: false
            }
        );
    }

    #[test]
    fn parse_rm_requires_id_or_all() {
        assert!(parse_args(["normen", "rm"]).is_err());
    }

    #[test]
    fn format_list_marks_menu_and_tabs() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = WorkspaceStore::open(dir.path().join("sessions.json"));
        store.save(
            None,
            -1,
            vec![
                crate::session::WorkspaceTab {
                    slug: "bgb".into(),
                    citation: "§ 433".into(),
                },
                crate::session::WorkspaceTab {
                    slug: "gg".into(),
                    citation: "Art 1".into(),
                },
            ],
        );
        let listing = format_list(&store);
        assert!(listing.contains("ID"), "{listing}");
        assert!(listing.contains('1'), "{listing}");
        assert!(listing.contains("MENU*"), "{listing}");
        assert!(listing.contains("BGB"), "{listing}");
        assert!(listing.contains("GG"), "{listing}");
    }

    #[test]
    fn apply_cli_rm_all_clears_store() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = WorkspaceStore::open(dir.path().join("sessions.json"));
        store.save(
            None,
            0,
            vec![crate::session::WorkspaceTab {
                slug: "bgb".into(),
                citation: "§ 1".into(),
            }],
        );
        apply_cli(
            &Command::Remove {
                id: None,
                all: true,
            },
            &mut store,
        )
        .unwrap();
        assert!(store.list().is_empty());
    }
}
