//! `lazyos theme` - the palette's only writer.
//!
//! A direct command (no window): `theme set` renders every surface, writes it
//! atomically, records what it wrote and asks the session to reload. The cycle
//! keys run exactly this with `next`, which is why they raise no card of their
//! own.
//!
//! Two of the exits are load-bearing, because `check-system.sh` and the key
//! bindings read them (spec 001): **0** did something, **1** nothing happened
//! (nothing to change, or a hand edit was not confirmed), **2** usage, **3**
//! failed and the reason is already on stderr.

use std::path::PathBuf;

use lazyos_theme::store::{self, Outcome, Paths};
use lazyos_theme::{SURFACES, Theme, builtin, wallpaper};

use crate::command::Resolved;

/// `println!` panics when the reader closes the pipe (`lazyos theme list | head`),
/// which is a normal way to read a list. A write that fails is not worth a
/// panic in a command whose job is to print a few lines.
macro_rules! say {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

const EXIT_OK: i32 = 0;
const EXIT_NOTHING: i32 = 1;
const EXIT_USAGE: i32 = 2;
const EXIT_FAILED: i32 = 3;

/// The paths this command owns: `$HOME`, and the state directory the XDG spec
/// names (`$XDG_STATE_HOME/lazyos/theme`).
pub fn paths() -> Paths {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let state = match std::env::var("XDG_STATE_HOME") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir).join("lazyos/theme"),
        _ => PathBuf::from(&home).join(".local/state/lazyos/theme"),
    };
    Paths::new(home, state)
}

pub fn run(resolved: &Resolved) -> i32 {
    let paths = paths();
    let args: Vec<&str> = resolved.args.iter().map(String::as_str).collect();
    match args.split_first() {
        None => list(&paths),
        Some((&"list" | &"ls", [])) => list(&paths),
        Some((&"current", [])) => current(&paths),
        Some((&"validate", rest)) => validate(rest),
        Some((&"preview", rest)) => preview(&paths, rest),
        Some((&"next", rest)) => next(&paths, rest),
        Some((&"wallpaper", rest)) if *rest == ["next"] => wallpaper_next(&paths),
        Some((&"diff", rest)) => diff(&paths, rest),
        Some((&"render", rest)) => render_into(rest),
        Some((&"set", rest)) => set(&paths, rest),
        Some((&"help" | &"--help" | &"-h", _)) => usage(),
        Some((other, _)) => {
            eprintln!("lazyos theme: '{other}' is not a subcommand");
            usage()
        }
    }
}

fn usage() -> i32 {
    eprintln!(
        "usage: lazyos theme [list | set NAME [--force] [--headless] | next | current | validate [NAME...]]"
    );
    eprintln!();
    eprintln!("  list       every theme, with the active one marked");
    eprintln!("  set NAME   generate every surface from NAME and reload the session");
    eprintln!("  next       the next theme in the cycle (Alt+Shift+T)");
    eprintln!("  current    the active theme, as `slug<TAB>name`");
    eprintln!("  validate   parse and check the built-ins (or the names given)");
    eprintln!("  preview [NAME]  the palette as a card: roles, the 16 ANSI swatches, the ratios");
    eprintln!("  wallpaper next  the next wallpaper of the active theme (Alt+Shift+W)");
    eprintln!(
        "  diff [--quiet]  compare every surface with a fresh render (the idempotence witness)"
    );
    eprintln!("  render --to DIR [--home PATH] [--theme NAME]  render every surface into DIR");
    eprintln!();
    eprintln!("  --force    replace a file a person edited");
    eprintln!("  --headless write the files only (no swaymsg/waybar/mako reload)");
    EXIT_USAGE
}

pub fn theme(slug: &str) -> Result<Theme, String> {
    let text = builtin::get(slug).ok_or_else(|| {
        format!(
            "'{slug}' is not a theme: {}",
            builtin::slugs().collect::<Vec<_>>().join(", ")
        )
    })?;
    Theme::parse(text)
}

fn list(paths: &Paths) -> i32 {
    let active = store::active(paths);
    for (slug, text) in builtin::ALL {
        let name = Theme::parse(text)
            .map(|theme| theme.name)
            .unwrap_or_else(|_| slug.to_string());
        let mark = if active.as_deref() == Some(slug) {
            "*"
        } else {
            " "
        };
        say!("{mark} {slug:<12} {name}");
    }
    EXIT_OK
}

fn current(paths: &Paths) -> i32 {
    match store::active(paths) {
        Some(slug) => {
            let name = theme(&slug).map(|theme| theme.name).unwrap_or_default();
            say!("{slug}\t{name}");
            EXIT_OK
        }
        None => {
            eprintln!("lazyos theme: no theme is active (run `lazyos theme set solarized`)");
            EXIT_NOTHING
        }
    }
}

fn validate(names: &[&str]) -> i32 {
    let names: Vec<&str> = if names.is_empty() {
        builtin::slugs().collect()
    } else {
        names.to_vec()
    };
    let mut failed = 0;
    for slug in names {
        match theme(slug) {
            Ok(theme) => say!("ok\t{slug}\t{}", theme.name),
            Err(error) => {
                failed += 1;
                say!("fail\t{slug}\t{error}");
            }
        }
    }
    if failed == 0 { EXIT_OK } else { EXIT_FAILED }
}

/// The palette as a card, in the terminal it is printed in.
///
/// A preview, not a window: truecolour blocks are what the theme will look like,
/// they work over ssh, and nothing has to be installed to see them. The ratios
/// are the same function `validate` refuses a theme with, so the card explains
/// the refusal it would get.
fn preview(paths: &Paths, rest: &[&str]) -> i32 {
    let slug = match rest.iter().find(|arg| !arg.starts_with("--")) {
        Some(slug) => (*slug).to_string(),
        None => match store::active(paths) {
            Some(slug) => slug,
            None => "solarized".to_string(),
        },
    };
    let theme = match theme(&slug) {
        Ok(theme) => theme,
        Err(error) => {
            eprintln!("lazyos theme preview: {error}");
            return EXIT_USAGE;
        }
    };
    let swatch = |colour: lazyos_theme::Hex| {
        let c = colour.0;
        format!("\x1b[48;2;{};{};{}m    \x1b[0m", c.r, c.g, c.b)
    };
    say!("{} ({})", theme.name, slug);
    if !theme.author.is_empty() {
        say!("by {}", theme.author);
    }
    if !theme.source.is_empty() {
        say!("{}", theme.source);
    }
    say!();
    say!("roles");
    for (name, colour) in theme.roles().table() {
        say!("  {} {name:<14} {}", swatch(colour), colour.hashed());
    }
    say!();
    say!("the sixteen");
    let c = theme.colors;
    let ansi = [
        ("background", c.background),
        ("regular0", c.regular0),
        ("regular1", c.regular1),
        ("regular2", c.regular2),
        ("regular3", c.regular3),
        ("regular4", c.regular4),
        ("regular5", c.regular5),
        ("regular6", c.regular6),
        ("regular7", c.regular7),
        ("bright0", c.bright0),
        ("bright1", c.bright1),
        ("bright2", c.bright2),
        ("bright3", c.bright3),
        ("bright4", c.bright4),
        ("bright5", c.bright5),
        ("bright6", c.bright6),
        ("bright7", c.bright7),
    ];
    for row in ansi.chunks(8) {
        let cells: Vec<String> = row
            .iter()
            .map(|(_, colour)| format!("{} {}", swatch(*colour), colour.bare()))
            .collect();
        say!("  {}", cells.join("  "));
        let names: Vec<String> = row.iter().map(|(name, _)| format!("{name:<12}")).collect();
        say!("  {}", names.join("  "));
    }
    say!();
    say!("contrast against the background");
    let roles = theme.roles();
    for (name, floor) in [
        ("text", 4.5),
        ("accent", 3.0),
        ("danger", 3.0),
        ("warn", 3.0),
        ("ok", 3.0),
        ("info", 3.0),
    ] {
        let colour = match name {
            "text" => roles.text,
            "accent" => roles.accent,
            "danger" => roles.danger,
            "warn" => roles.warn,
            "ok" => roles.ok,
            _ => roles.info,
        };
        let ratio = lazyos_theme::contrast::ratio(colour.0, c.background.0);
        let verdict = if ratio >= floor { "ok" } else { "BELOW FLOOR" };
        say!("  {name:<8} {ratio:>5.1}:1  (floor {floor:.1})  {verdict}");
    }
    EXIT_OK
}

fn next(paths: &Paths, rest: &[&str]) -> i32 {
    let slug = store::active(paths).unwrap_or_else(|| "solarized".to_string());
    let slugs: Vec<&str> = builtin::slugs().collect();
    let at = slugs
        .iter()
        .position(|candidate| *candidate == slug)
        .unwrap_or(0);
    let target = slugs[(at + 1) % slugs.len()];
    let mut args = vec![target];
    args.extend_from_slice(rest);
    set(paths, &args)
}

pub fn apply_theme(
    paths: &Paths,
    slug: &str,
    force: bool,
    headless: bool,
) -> Result<(Theme, usize, usize), String> {
    let theme = theme(slug)?;

    // Generate the wallpaper and thumbnail for the theme (spec 013).
    let _ = wallpaper::get_or_generate(
        &theme,
        slug,
        lazyos_theme::wallpaper::DEFAULT_WIDTH,
        lazyos_theme::wallpaper::DEFAULT_HEIGHT,
    );
    let _ = wallpaper::generate_thumbnail(&theme, slug);

    let mut written = 0;
    let mut unchanged = 0;
    let mut edited: Vec<&str> = Vec::new();
    let mut failed = 0;
    for surface in &SURFACES {
        match store::install(paths, surface, &theme, slug, force) {
            Ok(Outcome::Written) => written += 1,
            Ok(Outcome::Unchanged) => unchanged += 1,
            Ok(Outcome::Edited) => {
                edited.push(surface.name);
                let target = paths.target(surface);
                let body =
                    store::render_surface(surface, &theme, slug, &paths.home.to_string_lossy())
                        .unwrap_or_default();
                let current = std::fs::read_to_string(&target).unwrap_or_default();
                eprintln!(
                    "lazyos theme: {} was edited since it was generated:\n{}",
                    target.display(),
                    store::diff_summary(&current, &body)
                );
            }
            Err(error) => {
                failed += 1;
                eprintln!("lazyos theme: {error}");
            }
        }
    }
    if failed > 0 {
        return Err("one or more surfaces failed to install".to_string());
    }
    if !edited.is_empty() {
        return Err(format!(
            "{} file(s) left alone: {}. Re-run with --force to replace them.",
            edited.len(),
            edited.join(", ")
        ));
    }
    if let Err(error) = store::set_active(paths, slug) {
        return Err(error);
    }
    if !headless {
        reload();
    }
    Ok((theme, written, unchanged))
}

fn set(paths: &Paths, args: &[&str]) -> i32 {
    let force = args.contains(&"--force");
    let headless = args.contains(&"--headless");
    let Some(slug) = args.iter().find(|arg| !arg.starts_with("--")) else {
        eprintln!("lazyos theme set: which theme?");
        return usage();
    };
    let slug = *slug;
    match apply_theme(paths, slug, force, headless) {
        Ok((theme, written, unchanged)) => {
            say!(
                "{slug}\t{}\t{written} written, {unchanged} unchanged",
                theme.name
            );
            EXIT_OK
        }
        Err(error) => {
            if error.contains("file(s) left alone") {
                eprintln!("lazyos theme: {error}");
                EXIT_NOTHING
            } else {
                eprintln!("lazyos theme set: {error}");
                EXIT_FAILED
            }
        }
    }
}

/// Asks the session to pick the new files up.
///
/// None of this is a daemon: `swaymsg reload` re-reads the configuration (which
/// re-runs the `exec` lines that read a palette), waybar re-reads its stylesheet
/// on SIGUSR2 and mako on `makoctl reload`. A missing program is a line, not a
/// failure - a theme set over ssh has nothing to reload.
pub fn reload() {
    if std::env::var("WAYLAND_DISPLAY").is_err() {
        return;
    }
    for (program, args) in [
        ("swaymsg", vec!["reload"]),
        ("makoctl", vec!["reload"]),
        ("pkill", vec!["-USR2", "-x", "waybar"]),
    ] {
        match std::process::Command::new(program).args(&args).output() {
            Ok(output) if output.status.success() => {}
            Ok(_) => eprintln!(
                "lazyos theme: {program} {} reported nothing to do",
                args.join(" ")
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => eprintln!("lazyos theme: cannot run {program}: {error}"),
        }
    }
}

/// The idempotence witness: render every surface again and compare it with what
/// is installed. A non-empty list is exit 1, because a generated file that does
/// not match its template means either a hand edit or a stale image - and both
/// are worth a line in the system report.
fn diff(paths: &Paths, args: &[&str]) -> i32 {
    let quiet = args.contains(&"--quiet");
    let Some(slug) = store::active(paths) else {
        eprintln!("lazyos theme: no theme is active");
        return EXIT_NOTHING;
    };
    let Ok(theme) = theme(&slug) else {
        eprintln!("lazyos theme: the active theme `{slug}` is not built in");
        return EXIT_FAILED;
    };
    let home = paths.home.to_string_lossy().to_string();
    let mut differing = Vec::new();
    for surface in &SURFACES {
        let Ok(body) = store::render_surface(surface, &theme, &slug, &home) else {
            differing.push(surface.name);
            continue;
        };
        let installed = std::fs::read_to_string(paths.target(surface)).unwrap_or_default();
        if installed != body {
            differing.push(surface.name);
            if !quiet {
                say!(
                    "differs\t{}\n{}",
                    surface.name,
                    store::diff_summary(&installed, &body)
                );
            }
        }
    }
    if differing.is_empty() {
        if !quiet {
            say!("ok\t{slug}\t{} surfaces", SURFACES.len());
        }
        EXIT_OK
    } else {
        if quiet {
            say!("{}", differing.join(" "));
        }
        EXIT_NOTHING
    }
}

/// Renders every surface into a directory, for the host tool that commits the
/// solarized output (`scripts/make-theme.sh`) and for nothing else: no state, no
/// active theme, no reload.
fn render_into(args: &[&str]) -> i32 {
    let value = |flag: &str| {
        args.iter()
            .position(|arg| *arg == flag)
            .and_then(|at| args.get(at + 1))
            .copied()
    };
    let (Some(to), Some(home)) = (value("--to"), value("--home")) else {
        eprintln!("lazyos theme render: --to DIR and --home PATH are required");
        return EXIT_USAGE;
    };
    let slug = value("--theme").unwrap_or("solarized");
    let theme = match theme(slug) {
        Ok(theme) => theme,
        Err(error) => {
            eprintln!("lazyos theme render: {error}");
            return EXIT_USAGE;
        }
    };
    for surface in &SURFACES {
        let body = match store::render_surface(surface, &theme, slug, home) {
            Ok(body) => body,
            Err(error) => {
                eprintln!("lazyos theme render: {error}");
                return EXIT_FAILED;
            }
        };
        let target = PathBuf::from(to).join(surface.target);
        if let Err(error) = store::write_atomic(&target, &body) {
            eprintln!("lazyos theme render: {error}");
            return EXIT_FAILED;
        }
    }
    say!("{slug}\t{}\t{} surfaces", theme.name, SURFACES.len());
    EXIT_OK
}

/// Cycle the wallpaper for `for_slug` (or active theme if None).
pub fn cycle_wallpaper_for(
    paths: &Paths,
    for_slug: Option<&str>,
) -> Result<(String, &'static str), String> {
    let slug = match for_slug {
        Some(s) => s.to_string(),
        None => store::active(paths).ok_or_else(|| "no theme is active".to_string())?,
    };
    let theme = theme(&slug)?;
    if theme.wallpapers.len() < 2 {
        return Err(format!(
            "{} names one wallpaper, so there is nothing to cycle",
            theme.name
        ));
    }
    let state = paths.state.join("wallpaper");
    let current = std::fs::read_to_string(&state).unwrap_or_default();
    let current = current.trim();
    let at = theme
        .wallpapers
        .iter()
        .position(|name| name == current)
        .unwrap_or(usize::MAX);
    let next = if at == usize::MAX {
        0
    } else {
        (at + 1) % theme.wallpapers.len()
    };
    let mut theme = theme;
    theme.wallpapers.rotate_left(next);
    let surface = SURFACES
        .iter()
        .find(|surface| surface.name == "sway")
        .expect("the wallpaper surface is in the table");
    store::install(paths, surface, &theme, &slug, true)?;
    let _ = std::fs::write(&state, &theme.wallpapers[0]);
    reload();
    Ok((theme.wallpapers[0].clone(), surface.name))
}

/// The wallpaper half of `Alt+Shift+W`: the next name in the active theme's
/// list, written through the same surface so it is regenerated and recorded.
pub fn wallpaper_next(paths: &Paths) -> i32 {
    match cycle_wallpaper_for(paths, None) {
        Ok((wp, surf)) => {
            say!("{wp}\t{surf}");
            EXIT_OK
        }
        Err(error) => {
            eprintln!("lazyos theme: {error}");
            if error.contains("no theme is active") || error.contains("names one wallpaper") {
                EXIT_NOTHING
            } else {
                EXIT_FAILED
            }
        }
    }
}
