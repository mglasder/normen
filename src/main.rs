use std::process::ExitCode;

use normen::catalog::resolve_core;
use normen::cli::{apply_cli, parse_args, Command};
use normen::config::{default_config_path, Config};
use normen::fetch::{default_cache_dir, download_law_xml, load_law};
use normen::query::{run_print, QueryFail};
use normen::session::WorkspaceStore;
use normen::ui::{App, Start};

struct Fail {
    message: String,
    exit: u8,
}

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self { message, exit: 1 }
    }
}

impl From<QueryFail> for Fail {
    fn from(fail: QueryFail) -> Self {
        Self {
            message: fail.message,
            exit: fail.exit,
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(fail) => {
            eprint!("{}", fail.message);
            if !fail.message.ends_with('\n') {
                eprintln!();
            }
            ExitCode::from(fail.exit)
        }
    }
}

fn run() -> Result<(), Fail> {
    let (command, flags) = parse_args(std::env::args_os())?;
    let config = Config::new(default_config_path());
    config.ensure_file();
    let cache_dir = default_cache_dir();
    let session_path = cache_dir.join("sessions.json");
    match command {
        Command::List | Command::Remove { .. } => {
            let mut store = WorkspaceStore::open(&session_path);
            let out = apply_cli(&command, &mut store)?;
            if !out.is_empty() {
                println!("{out}");
            }
            Ok(())
        }
        Command::Attach { id } => {
            let cache = cache_dir.clone();
            let mut app = App::start_with_path(
                config,
                Box::new(move |law_ref, refresh| {
                    load_law(&cache, download_law_xml, law_ref, refresh)
                }),
                Start::Attach { id },
                flags.refresh,
                session_path,
            )?;
            app.run().map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Open => {
            let cache = cache_dir.clone();
            let mut app = App::start_with_path(
                config,
                Box::new(move |law_ref, refresh| {
                    load_law(&cache, download_law_xml, law_ref, refresh)
                }),
                Start::Fresh {
                    law: None,
                    norm: None,
                },
                flags.refresh,
                session_path,
            )?;
            app.run().map_err(|err| err.to_string())?;
            Ok(())
        }
        Command::Query { .. } | Command::Laws { .. } => {
            let mut index = normen::bundesrecht::load_cache(&cache_dir);
            let order = config.core_order();
            let (mut core, mut warnings) = resolve_core(order.as_deref(), &index);
            if !warnings.is_empty() {
                if let Ok(fetched) =
                    normen::bundesrecht::fetch_index(normen::bundesrecht::download_teilliste)
                {
                    let _ = normen::bundesrecht::save_cache(&cache_dir, &fetched);
                    index = fetched;
                    let resolved = resolve_core(order.as_deref(), &index);
                    core = resolved.0;
                    warnings = resolved.1;
                }
            }
            if !warnings.is_empty() {
                eprint!("unknown law {}\n", warnings.join(", "));
            }
            let cache = cache_dir.clone();
            let json = run_print(&command, &flags, &core, |law_ref, refresh| {
                load_law(&cache, download_law_xml, law_ref, refresh)
            })?;
            println!("{json}");
            Ok(())
        }
    }
}
