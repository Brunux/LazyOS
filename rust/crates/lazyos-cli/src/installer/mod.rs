//! The full TUI installer (spec 018): account setup, localization, disk safety, and progress.

pub mod config;
pub mod preflight;
pub mod ui;
pub mod worker;

use std::io;

use lazyos_ui::app::App;
use lazyos_ui::design::Design;
use lazyos_ui::widgets::Header;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::Failure;
use preflight::Preflight;
use ui::InstallerScreen;

/// Run the interactive TUI installer wizard.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    design: &Design,
    dry_run: bool,
) -> Result<i32, Failure> {
    let preflight = Preflight::from_machine();
    let mut screen = InstallerScreen::new(preflight, dry_run);

    let mut app = App::new(Header::new("LazyOS").subtitle("System Installer"))
        .design(design.clone())
        .screen(&mut screen);

    app.run(terminal)
        .map_err(|err| Failure::new("Installer terminal session failed", err.to_string()))
}

/// Headless `--check` flag: evaluate eMMC safety invariants, target discovery, and tool prerequisites.
pub fn check() -> i32 {
    let preflight = Preflight::from_machine();
    let (ok, summary) = preflight.check_summary();
    println!("{summary}");
    if ok {
        lazyos_ui::app::exit::OK
    } else {
        lazyos_ui::app::exit::ESCAPE
    }
}

/// Headless `apply` command: execute detached installer worker with given config JSON.
pub fn apply(args: &[String]) -> i32 {
    let config = if let Some(path) = args.first() {
        if let Ok(contents) = std::fs::read_to_string(path) {
            serde_json::from_str(&contents).unwrap_or_default()
        } else {
            config::InstallConfig::default()
        }
    } else {
        config::InstallConfig::default()
    };

    match worker::run_worker(config, None) {
        Ok(()) => lazyos_ui::app::exit::OK,
        Err(err) => {
            eprintln!("installer worker failed: {err}");
            lazyos_ui::app::exit::FAILED
        }
    }
}
