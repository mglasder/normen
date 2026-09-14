use std::ffi::OsString;

use clap::error::ErrorKind;
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

use crate::catalog::resolve_law;
use crate::session::WorkspaceStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuerySpec {
    Outline,
    Get(String),
    Search(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LawSort {
    Priority,
    Alpha,
    Rev,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Open,
    Query { law: String, spec: QuerySpec },
    Laws {
        filter: Option<String>,
        sort: LawSort,
    },
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

const ROOT_AFTER_HELP: &str = "\
Ohne Gesetz: interaktives TUI (Terminal nötig).
Mit Gesetz oder »laws«: pretty JSON auf stdout, kein TUI, kein »Lade …«.

  normen laws [filter]     CORE { query, laws: [{ shortcut, slug, title }] }
  normen BGB               Gliederung { law, norms: [{ citation, title }] }
  normen BGB 433           Norm { law, citation, title, text }
  normen BGB /kauf         Suche { law, query, limit, total, hits }

Zweitargument: Zitat (433, 31a, § 433) oder /Suchwort.
Ohne Schrägstrich ist »Kaufvertrag« kein Zitat (Exit 2).
Unbekanntes Gesetz oder fehlendes Zitat: Exit 2, Fehler auf stderr.
Sonst Exit 1. Leere Suche oder leerer Katalogfilter: Exit 0, leeres Array.

--limit gilt nur für /Suche (Standard 10). --all hebt die Kappe auf.
--limit 0 und --all zusammen mit --limit sind ungültig.";

const LAWS_AFTER_HELP: &str = "\
  normen laws                 CORE in Prioritätsreihenfolge
  normen laws bürger          Filter über Kürzel, Slug, Titel oder Alias
  normen laws --sort alpha    Kürzel A–Z
  normen laws --sort rev      Kürzel Z–A
  normen laws --sort priority Priorität (conf / Standard, Vorgabe)

Ausgabe: { query, laws: [{ shortcut, slug, title }] }
Leerer Filter: Exit 0, laws: []. --limit/--all/--refresh wirken nicht.";

#[derive(Parser)]
#[command(
    name = "normen",
    about = "Deutsche Gesetze lesen und durchsuchen (gesetze-im-internet.de).",
    after_help = ROOT_AFTER_HELP,
    override_usage = "normen [OPTIONS] [LAW] [NORM]\n       normen [OPTIONS] <COMMAND>",
    disable_help_subcommand = true
)]
struct Cli {
    /// Gesetzestexte neu von gesetze-im-internet.de laden
    #[arg(long, global = true)]
    refresh: bool,
    /// Trefferzahl bei /Suche (Standard 10)
    #[arg(long, global = true)]
    limit: Option<u32>,
    /// Alle Suchtreffer ausgeben (kein Limit)
    #[arg(long, global = true, action = clap::ArgAction::SetTrue)]
    all: bool,
    #[command(subcommand)]
    command: Option<SubCommand>,
    /// Gesetzeskürzel oder Slug (BGB, gg, bbaug)
    law: Option<String>,
    /// Normzitat (433, 31a) oder /Volltextsuche
    norm: Option<String>,
}

#[derive(Subcommand)]
enum SubCommand {
    /// Gespeicherten Workspace fortsetzen
    #[command(
        about = "Letzten oder angegebenen Workspace im TUI öffnen.",
        after_help = "Braucht ein Terminal. Ohne ID: zuletzt gespeicherter Workspace."
    )]
    Attach {
        /// Workspace-Nummer (normen list)
        id: Option<u32>,
    },
    /// Gespeicherte Workspaces auflisten
    #[command(about = "Gespeicherte Workspaces auflisten (kein JSON-Gesetzestext).")]
    List,
    /// CORE der verfügbaren Gesetze (JSON)
    #[command(
        about = "CORE der verfügbaren Gesetze als JSON.",
        after_help = LAWS_AFTER_HELP
    )]
    Laws {
        /// Teilstring für Kürzel, Slug, Titel oder Alias
        filter: Option<String>,
        /// Reihenfolge: priority (Vorgabe), alpha, rev
        #[arg(long, default_value = "priority", value_parser = ["priority", "alpha", "rev"])]
        sort: String,
    },
    /// Gespeicherte Workspaces löschen
    #[command(
        about = "Gespeicherte Workspaces löschen.",
        after_help = "normen rm 3   oder   normen rm --all"
    )]
    Rm {
        /// Workspace-Nummer
        id: Option<u32>,
        /// Alle Workspaces löschen
        #[arg(long)]
        all: bool,
    },
}

pub fn help_text(subcommand: Option<&str>) -> String {
    let mut cmd = command_with_hidden_globals(subcommand);
    match subcommand {
        None => cmd.render_long_help().to_string(),
        Some(name) => cmd
            .find_subcommand_mut(name)
            .unwrap_or_else(|| panic!("unknown subcommand {name}"))
            .render_long_help()
            .to_string(),
    }
}

fn command_with_hidden_globals(subcommand: Option<&str>) -> clap::Command {
    let mut cmd = Cli::command();
    for id in hidden_globals(subcommand) {
        cmd = cmd.mut_arg(id, |arg| arg.hide(true));
    }
    cmd
}

fn hidden_globals(subcommand: Option<&str>) -> &'static [&'static str] {
    match subcommand {
        Some("laws" | "list") => &["refresh", "limit", "all"],
        Some("rm") => &["refresh", "limit", "all"],
        Some("attach") => &["limit", "all"],
        _ => &[],
    }
}

fn peek_subcommand<T: AsRef<std::ffi::OsStr>>(args: &[T]) -> Option<&'static str> {
    const NAMES: &[&str] = &["attach", "list", "laws", "rm"];
    let mut skip_value = false;
    for arg in args.iter().skip(1) {
        let Some(text) = arg.as_ref().to_str() else {
            skip_value = false;
            continue;
        };
        if skip_value {
            skip_value = false;
            continue;
        }
        if text == "--limit" || text == "--sort" {
            skip_value = true;
            continue;
        }
        if let Some(name) = NAMES.iter().copied().find(|name| *name == text) {
            return Some(name);
        }
    }
    None
}

pub fn parse_args<I, T>(args: I) -> Result<(Command, CliFlags), String>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let argv: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let asking_help = argv.iter().any(|arg| arg == "--help" || arg == "-h");
    let sub = peek_subcommand(&argv);
    let cmd = if asking_help {
        command_with_hidden_globals(sub)
    } else {
        Cli::command()
    };
    let cli = match cmd.try_get_matches_from(&argv) {
        Ok(matches) => match Cli::from_arg_matches(&matches) {
            Ok(cli) => cli,
            Err(err) => return Err(err.to_string()),
        },
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
        Some(SubCommand::Laws { filter, sort }) => Command::Laws {
            filter,
            sort: parse_law_sort(&sort)?,
        },
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

fn parse_law_sort(raw: &str) -> Result<LawSort, String> {
    match raw {
        "priority" => Ok(LawSort::Priority),
        "alpha" => Ok(LawSort::Alpha),
        "rev" => Ok(LawSort::Rev),
        other => Err(format!("unknown --sort {other}")),
    }
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
            Command::Laws {
                filter: None,
                sort: LawSort::Priority,
            }
        );
        assert_eq!(
            parse_args(["normen", "laws", "bürger"]).unwrap().0,
            Command::Laws {
                filter: Some("bürger".into()),
                sort: LawSort::Priority,
            }
        );
    }

    #[test]
    fn parse_laws_sort_flags() {
        assert_eq!(
            parse_args(["normen", "laws", "--sort", "alpha"]).unwrap().0,
            Command::Laws {
                filter: None,
                sort: LawSort::Alpha,
            }
        );
        assert_eq!(
            parse_args(["normen", "laws", "--sort", "rev", "bgb"]).unwrap().0,
            Command::Laws {
                filter: Some("bgb".into()),
                sort: LawSort::Rev,
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
    fn parse_refresh_after_attach_is_global() {
        let (cmd, flags) = parse_args(["normen", "attach", "--refresh"]).unwrap();
        assert_eq!(cmd, Command::Attach { id: None });
        assert!(flags.refresh);
    }

    #[test]
    fn parse_refresh_before_attach_is_global() {
        let (cmd, flags) = parse_args(["normen", "--refresh", "attach"]).unwrap();
        assert_eq!(cmd, Command::Attach { id: None });
        assert!(flags.refresh);
    }

    #[test]
    fn parse_limit_after_laws_is_global() {
        let (cmd, flags) = parse_args(["normen", "laws", "--limit", "5"]).unwrap();
        assert_eq!(cmd, Command::Laws { filter: None, sort: LawSort::Priority });
        assert_eq!(flags.limit, Some(5));
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
            None,
        );
        let listing = format_list(&store);
        assert!(listing.contains("ID"), "{listing}");
        assert!(listing.contains('1'), "{listing}");
        assert!(listing.contains("MENU*"), "{listing}");
        assert!(listing.contains("BGB"), "{listing}");
        assert!(listing.contains("GG"), "{listing}");
    }

    #[test]
    fn root_help_teaches_print_cli() {
        let help = help_text(None);
        assert!(help.contains("pretty JSON"), "{help}");
        assert!(help.contains("normen BGB /kauf"), "{help}");
        assert!(help.contains("Exit 2"), "{help}");
        assert!(help.contains("Ohne Schrägstrich"), "{help}");
        assert!(help.contains("shortcut, slug, title"), "{help}");
    }

    #[test]
    fn laws_help_hides_search_flags() {
        let help = help_text(Some("laws"));
        assert!(
            !help.contains("Trefferzahl bei /Suche"),
            "{help}"
        );
        assert!(help.contains("shortcut, slug, title"), "{help}");
        assert!(help.contains("--sort"), "{help}");
        assert!(help.contains("priority"), "{help}");
    }

    #[test]
    fn rm_help_documents_workspace_all_not_search() {
        let help = help_text(Some("rm"));
        assert!(help.contains("Alle Workspaces löschen"), "{help}");
        assert!(!help.contains("Alle Suchtreffer"), "{help}");
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
            None,
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
