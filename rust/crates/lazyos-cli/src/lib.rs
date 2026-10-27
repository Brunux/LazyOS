//! `lazyos`, the session's multi-call binary.
//!
//! One binary answers to every name the session calls it by (`lazyos menu`,
//! `rofi-files`, …), decides where it should run, and owns the terminal and the
//! error story in one place. The commands themselves arrive in later specs; the
//! shape they plug into is this file.
//!
//! ```text
//! lazyos menu            the session menu     (spec 007: the real one)
//! lazyos menu files      the file browser     (spec 009: the real one)
//! lazyos --help          the table above, from the same data check-system reads
//! lazyos --version       the repository's VERSION, compiled in
//! ```
//!
//! Exit codes are the shell's contract (spec 001 §Design): `0` did something,
//! `1` Escape (nothing happened), `2` usage, `3` the action failed and the
//! reason was already shown - so `|| exit 0` in a binding stays correct.

pub mod command;
pub mod host;
pub mod paths;
pub mod task_cmd;
pub mod term;
mod theme;
pub mod version;

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::time::Duration;

use lazyos_ui::app::exit::{ESCAPE as EXIT_ESCAPE, FAILED as EXIT_FAILED};
use lazyos_ui::app::exit::{OK as EXIT_OK, USAGE as EXIT_USAGE};
use lazyos_ui::{
    App, BodyCtx, Design, EmptyScreen, FailureScreen, Header, KeyHint, Outcome, Screen,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use command::{COMMANDS, ResolveError, Resolved};
use host::{Env, Launch};

/// Why a command could not do what it was asked, in the two lines the card
/// shows: the summary says what failed, the detail says what to do about it.
pub struct Failure {
    pub summary: String,
    pub detail: String,
}

impl Failure {
    pub fn new(summary: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            detail: detail.into(),
        }
    }
}

/// The whole process, with everything it reads injected so the tests can drive
/// it without a session.
pub fn run(argv: Vec<String>, env: &Env) -> i32 {
    let argv0 = argv
        .first()
        .cloned()
        .unwrap_or_else(|| "lazyos".to_string());
    let args: Vec<String> = argv.iter().skip(1).cloned().collect();

    // The headless task runner: a menu starts a slow action by spawning
    // `lazyos --task-run <id>` detached. It has no terminal and no window, so it
    // is dispatched before the command table and never enters the terminal.
    if args.first().map(String::as_str) == Some("--task-run") {
        let id = args.get(1).cloned().unwrap_or_default();
        return task_cmd::run(&id);
    }

    if let Some(code) = global(&args) {
        return code;
    }

    let resolved = match command::resolve(&argv0, &args) {
        Ok(resolved) => resolved,
        Err(ResolveError::NoCommand) => {
            eprintln!("lazyos {}", version::version());
            eprintln!("usage: lazyos <command> [args]");
            eprintln!();
            print_commands(&mut io::stderr());
            return EXIT_USAGE;
        }
        Err(ResolveError::Unknown(name)) => {
            eprintln!("lazyos: unknown command '{name}'");
            print_commands(&mut io::stderr());
            return EXIT_USAGE;
        }
    };

    match host::plan(&resolved, env) {
        Launch::Direct => run_direct(&resolved),
        Launch::Inline => run_inline(&resolved),
        Launch::Spawn { program, args, .. } => match host::spawn(&program, &args) {
            Ok(()) => EXIT_OK,
            Err(error) => {
                eprintln!("lazyos: cannot open a window with {program}: {error}");
                eprintln!("lazyos: run it inside a terminal with LAZYOS_UI=1 to see the reason");
                EXIT_FAILED
            }
        },
        Launch::NoTerminal => {
            eprintln!("lazyos {}: no terminal to draw in", resolved.spec.path);
            eprintln!("lazyos: run it in a terminal, or from the session where `foot` exists");
            EXIT_FAILED
        }
    }
}

/// The flags that never resolve to a command: they are how a person (or
/// `check-system.sh`) reads the table without starting a UI.
fn global(args: &[String]) -> Option<i32> {
    let first = args.first().map(String::as_str)?;
    match first {
        "-V" | "--version" => {
            println!("{}", version::version());
            Some(EXIT_OK)
        }
        "-h" | "--help" => {
            let mut out = io::stdout();
            let _ = writeln!(out, "lazyos {} - the LazyOS session UI", version::version());
            let _ = writeln!(out);
            let _ = writeln!(out, "usage: lazyos <command> [args]");
            let _ = writeln!(out, "       <symlink name> [args]");
            let _ = writeln!(out);
            print_commands(&mut out);
            let _ = writeln!(out);
            let _ = writeln!(out, "options:");
            let _ = writeln!(out, "  -h, --help      this list");
            let _ = writeln!(
                out,
                "  -V, --version   the version this image was built with"
            );
            let _ = writeln!(
                out,
                "  --glyphs        the codepoints the windows draw, one per line"
            );
            let _ = writeln!(
                out,
                "  --glyph-font    the font the TUI alphabet was filtered against"
            );
            let _ = writeln!(
                out,
                "  --symlinks      the names the session calls this binary by"
            );
            Some(EXIT_OK)
        }
        // The codepoints the widgets draw, so `check-system.sh` can ask a font
        // for each one. A box-drawing glyph no font has is a hole in the frame
        // that logs nothing, which is why the list travels with the binary.
        //
        // Two alphabets, one list: the frame's box-drawing characters and the
        // Nerd Font names a row's picture comes from (spec 003). The name beside
        // a codepoint is what the report calls a hole.
        "--glyphs" => {
            let mut out = io::stdout();
            let mut seen = BTreeMap::new();
            for glyph in lazyos_ui::GLYPHS {
                seen.insert(*glyph as u32, "frame");
            }
            for glyph in lazyos_ui::Glyph::all() {
                if let Some(ch) = glyph.codepoint() {
                    seen.entry(ch as u32).or_insert(glyph.name());
                }
            }
            for (codepoint, name) in seen {
                let _ = writeln!(out, "U+{codepoint:04X}\t{name}");
            }
            Some(EXIT_OK)
        }
        // The pin the alphabet was generated against, for the witness: the check
        // compares it with the font installed on the machine, because a font
        // upgrade that drops a codepoint turns that row into an empty box.
        "--glyph-font" => {
            let mut out = io::stdout();
            let _ = writeln!(out, "family={}", lazyos_ui::glyphs::FONT_FAMILY);
            let _ = writeln!(out, "file={}", lazyos_ui::glyphs::FONT_FILE);
            let _ = writeln!(out, "sha256={}", lazyos_ui::glyphs::FONT_SHA256);
            let _ = writeln!(out, "nerd={}", lazyos_ui::glyphs::NERD_VERSION);
            let _ = writeln!(out, "drawn={}", lazyos_ui::glyphs::DRAWN);
            let _ = writeln!(out, "fallback={}", lazyos_ui::glyphs::FALLBACK);
            for glyph in lazyos_ui::Glyph::all() {
                if glyph.codepoint().is_none() {
                    let _ = writeln!(out, "text={} {}", glyph.name(), glyph.token());
                }
            }
            Some(EXIT_OK)
        }
        // Machine-readable, for the witness: the links the installer creates
        // today, from the same table `--help` prints.
        "--symlinks" => {
            let mut out = io::stdout();
            for (name, path) in command::installed_symlinks() {
                let _ = writeln!(out, "{name}\t{path}");
            }
            Some(EXIT_OK)
        }
        _ => None,
    }
}

fn print_commands(out: &mut impl Write) {
    let _ = writeln!(out, "commands:");
    for spec in COMMANDS {
        let _ = writeln!(out, "  {:<14} {}", spec.path, spec.summary);
    }
}

/// Draw in the terminal this process already has.
/// A command with no surface: it writes its files and says what it did, so it
/// runs where it was called - a `foot` window for a `mv` would steal the focus.
fn run_direct(resolved: &Resolved) -> i32 {
    match resolved.spec.path {
        "theme" => theme::run(resolved),
        other => {
            eprintln!("lazyos {other}: this command has no direct handler");
            EXIT_USAGE
        }
    }
}

fn run_inline(resolved: &Resolved) -> i32 {
    let session = match term::Session::enter() {
        Ok(session) => session,
        Err(error) => {
            eprintln!("lazyos: cannot use this terminal: {error}");
            return EXIT_FAILED;
        }
    };
    let mut terminal = match Terminal::new(CrosstermBackend::new(io::stdout())) {
        Ok(terminal) => terminal,
        Err(error) => {
            eprintln!("lazyos: cannot size this terminal: {error}");
            return EXIT_FAILED;
        }
    };
    let design = Design::for_session(std::env::var("XDG_RUNTIME_DIR").ok().as_deref());

    let code = match execute(resolved, &mut terminal, &design) {
        Ok(code) => code,
        Err(failure) => {
            show_failure(&mut terminal, &design, &failure);
            EXIT_FAILED
        }
    };

    // The terminal is restored by `session`'s Drop after `terminal` goes away.
    drop(session);
    code
}

fn execute(
    resolved: &Resolved,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    design: &Design,
) -> Result<i32, Failure> {
    match resolved.spec.path {
        "menu" => menu(resolved, terminal, design, "the session menu", "spec 007"),
        "menu files" => menu(resolved, terminal, design, "the file browser", "spec 009"),
        "task attach" => task_cmd::attach(resolved, terminal, design),
        other => Err(Failure::new(
            format!("`{other}` is in the table but has no implementation"),
            "this is a bug: the command table and this match disagree",
        )),
    }
}

/// The one command spec 001 ships: it proves the host, the frame and the keys.
///
/// It draws, it closes on Escape and it writes nothing - no state directory is
/// created, which is the test for "a menu is a process, not a daemon".
fn menu(
    resolved: &Resolved,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    design: &Design,
    title: &str,
    arrives: &str,
) -> Result<i32, Failure> {
    if let Some(extra) = resolved.args.first() {
        return Err(Failure::new(
            format!("`{extra}` is not something this menu knows"),
            format!("try: lazyos {}", resolved.spec.path),
        ));
    }
    let hint = format!("{arrives} - Esc closes");
    let mut screen = MenuScreen {
        inner: EmptyScreen::new(title, hint),
    };
    let mut app = App::new(Header::new("LazyOS").subtitle(title))
        .design(design.clone())
        .keybar(vec![
            KeyHint::new("Esc", "close"),
            KeyHint::new("q", "close"),
        ])
        .screen(&mut screen);
    app.run(terminal)
        .map_err(|error| Failure::new("the terminal stopped answering", error.to_string()))
}

/// The menu's body, and the one place a panic can be forced on purpose.
///
/// The acceptance test sets `LAZYOS_FORCE_PANIC=1` to prove the hook restores
/// the cursor and the alternate screen; the panic has to happen while the
/// terminal is in UI mode, which is what drawing the body is.
struct MenuScreen {
    inner: EmptyScreen,
}

impl Screen for MenuScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        if std::env::var_os("LAZYOS_FORCE_PANIC").is_some() {
            panic!("LAZYOS_FORCE_PANIC: the frame's panic path");
        }
        self.inner.draw(ctx)
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
            return Outcome::Done(EXIT_ESCAPE);
        }
        self.inner.key(key, design)
    }
}

/// The error card: what failed, and the one thing to try next.
///
/// It waits for a key because a card that closes itself is a card nobody read,
/// but never for ever - a broken input must not be able to hang the session.
fn show_failure(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    design: &Design,
    failure: &Failure,
) {
    let mut screen = FailureScreen::new(failure.summary.clone(), failure.detail.clone());
    let mut app = App::new(Header::new("LazyOS").subtitle("failed"))
        .design(design.clone())
        .keybar(vec![KeyHint::new("any key", "close")])
        .screen(&mut screen);
    let _ = app.run_until(terminal, Some(Duration::from_secs(30)));
}
