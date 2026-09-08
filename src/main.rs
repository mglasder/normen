use clap::Parser;

use normen::config::{default_config_path, Config};
use normen::fetch::{default_cache_dir, load_cached_law};
use normen::ui::App;

#[derive(Parser)]
#[command(
    name = "normen",
    about = "Deutsche Gesetze lesen und durchsuchen (gesetze-im-internet.de)."
)]
struct Cli {
    /// Kürzel, z.B. BGB, GG, StGB, VwGO
    law: Option<String>,
    /// Normnummer (433, 31a) oder /Volltextsuche
    norm: Option<String>,
    /// Gesetzestexte neu von gesetze-im-internet.de laden
    #[arg(long)]
    refresh: bool,
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    let config = Config::new(default_config_path());
    config.ensure_file();
    if let Some(law) = cli.law.as_deref() {
        eprintln!("Lade {law} …");
    }
    let cache_dir = default_cache_dir();
    let mut app = App::new(
        config,
        Box::new(move |law_ref, refresh| load_cached_law(&cache_dir, law_ref, refresh)),
        cli.law,
        cli.norm,
        cli.refresh,
    );
    app.run()
}
