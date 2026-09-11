use std::process::ExitCode;

use normen::cli::{apply_cli, parse_args, Command};
use normen::config::{default_config_path, Config};
use normen::fetch::{default_cache_dir, download_law_xml, load_law};
use normen::session::WorkspaceStore;
use normen::ui::{App, Start};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprint!("{err}");
            if !err.ends_with('\n') {
                eprintln!();
            }
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let (command, refresh) = parse_args(std::env::args_os())?;
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
                refresh,
                session_path,
            )?;
            app.run().map_err(|err| err.to_string())
        }
        Command::Open { law, norm } => {
            if let Some(name) = law.as_deref() {
                eprintln!("Lade {name} …");
            }
            let cache = cache_dir.clone();
            let mut app = App::start_with_path(
                config,
                Box::new(move |law_ref, refresh| {
                    load_law(&cache, download_law_xml, law_ref, refresh)
                }),
                Start::Fresh { law, norm },
                refresh,
                session_path,
            )?;
            app.run().map_err(|err| err.to_string())
        }
    }
}
