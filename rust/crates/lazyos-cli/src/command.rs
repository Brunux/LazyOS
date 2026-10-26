//! The command table and the two ways a name reaches a command.
//!
//! Every surface the framework grows is a row here, and the row is what
//! `--help`, `--symlinks` and the `check-system.sh` witness read: one source of
//! truth, so a command that exists but is not listed (or a symlink that points
//! nowhere) is a test failure rather than a customer finding.
//!
//! Resolution has two entry points, and they must agree:
//!
//! - `lazyos menu files` - the words after the binary name, longest match first,
//!   so `menu` does not swallow `menu files`.
//! - a symlink called `rofi-files` (or `lazyos-menu`) - the file name, which is
//!   how the ~64 old paths keep working once each surface is ported.

use std::path::Path;

/// The window shape a command asks the host for (plan-ui §3).
///
/// The shape is data in the table, not a decision each command makes: the host
/// derives the `foot` app-id and the floating geometry from it, and sway's rules
/// can key on the same three names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A floating, centred menu sized for a list.
    Pick,
    /// A floating, centred window sized for a task (log, gauge, progress).
    Task,
    /// Fullscreen (the installer, the greeter).
    Full,
    /// No window at all: a command whose whole job is to write a file and
    /// print a line (`theme set`). A menu still gets a canvas; this one would
    /// be a terminal that opens and closes around a `mv`.
    Direct,
}

impl Shape {
    /// The app-id suffix (`foot --app-id=lazyos-pick`). This is the name
    /// `config.d/` and `swaymsg` criteria use.
    pub fn slug(self) -> &'static str {
        match self {
            Shape::Pick => "pick",
            Shape::Task => "task",
            Shape::Full => "full",
            Shape::Direct => "direct",
        }
    }

    pub fn app_id(self) -> String {
        format!("lazyos-{}", self.slug())
    }

    /// The shape's size hints, for the document that owns window geometry
    /// (`uis/sway/files/etc/sway/config.d/18-framework-windows.conf`). Sway
    /// applies them when it maps the surface; a `swaymsg` from this process
    /// would race that mapping (measured: the child runs before foot's first
    /// commit, so the criteria matched no node and the window stayed tiled).
    pub fn size(self) -> Option<(u16, u16)> {
        match self {
            Shape::Pick => Some((900, 600)),
            Shape::Task => Some((940, 620)),
            Shape::Full | Shape::Direct => None,
        }
    }
}

/// One row of the table.
#[derive(Debug, PartialEq, Eq)]
pub struct CommandSpec {
    /// The canonical words, as `lazyos <path>` spells them.
    pub path: &'static str,
    /// The one line `--help` prints.
    pub summary: &'static str,
    pub shape: Shape,
    /// Every name that resolves to this command, including ones whose old
    /// script still owns the name (they are installed by the spec that ports
    /// it, and `--symlinks` reports only the installed ones).
    pub symlinks: &'static [&'static str],
    /// The subset of `symlinks` the installer creates **today**. A name whose
    /// surface has not been ported stays out of this list so the old script
    /// keeps working; `check-system.sh` validates exactly this set.
    pub installed: &'static [&'static str],
}

/// The table. Adding a command here is what makes it callable by name, by
/// symlink and visible in `--help` at once.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        path: "menu",
        summary: "the session menu",
        shape: Shape::Pick,
        symlinks: &["lazyos-menu"],
        installed: &["lazyos-menu"],
    },
    CommandSpec {
        // The palette. `theme set` writes files and prints what it did, so it
        // never opens a window: the cycle keys run it from the session and a
        // window would steal the focus for a `mv`.
        path: "theme",
        summary: "list, set or check the session palette (spec 004)",
        shape: Shape::Direct,
        symlinks: &[],
        installed: &[],
    },
    CommandSpec {
        // A card from the command line (spec 006). It writes nothing and opens
        // no window: the notification daemon shows it, so it runs where it was
        // called, like `theme`.
        path: "notify",
        summary: "send a notification card: send, say, progress (spec 006)",
        shape: Shape::Direct,
        symlinks: &[],
        installed: &[],
    },
    CommandSpec {
        // The file browser (`rofi-files` today). Its own spec ports the
        // surface; until then both paths show the same empty state, which is
        // what keeps "one handler" true from the first commit.
        path: "menu files",
        summary: "browse files (spec 009)",
        shape: Shape::Pick,
        symlinks: &["rofi-files"],
        installed: &[],
    },
    CommandSpec {
        // The task surface: reattach to a slow action that outlives a toast.
        path: "task attach",
        summary: "reattach to a running or finished task (spec 005)",
        shape: Shape::Task,
        symlinks: &["lazyos-task"],
        installed: &["lazyos-task"],
    },
];

/// A command the table resolved, with the arguments that belong to it.
#[derive(Debug, PartialEq, Eq)]
pub struct Resolved {
    pub spec: &'static CommandSpec,
    pub args: Vec<String>,
}

impl Resolved {
    /// The words to pass to a child process so it resolves to the same command
    /// **without** the symlink: the host must hand `foot` the canonical path,
    /// not the name it was called by.
    pub fn words(&self) -> Vec<&'static str> {
        self.spec.path.split(' ').collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResolveError {
    /// The binary was called with no command at all.
    NoCommand,
    /// A name nothing in the table answers to.
    Unknown(String),
}

/// Resolve `argv[0]` (which may be a symlink name) and the words after it.
pub fn resolve(argv0: &str, args: &[String]) -> Result<Resolved, ResolveError> {
    let called_as = basename(argv0);

    // 1. The file name wins: a symlink is an explicit name for one command, and
    //    `lazyos` itself is not in any row's `symlinks`.
    for spec in COMMANDS {
        if spec.symlinks.contains(&called_as.as_str()) {
            return Ok(Resolved {
                spec,
                args: args.to_vec(),
            });
        }
    }

    // 2. The words after `lazyos`, longest match first (`menu files` before
    //    `menu`, so a sub-surface is not swallowed by its parent).
    if !args.is_empty() {
        let mut best: Option<(usize, &'static CommandSpec)> = None;
        for spec in COMMANDS {
            let words: Vec<&str> = spec.path.split(' ').collect();
            if words.len() <= args.len() && args[..words.len()] == words[..] {
                match best {
                    Some((len, _)) if len >= words.len() => {}
                    _ => best = Some((words.len(), spec)),
                }
            }
        }
        if let Some((len, spec)) = best {
            return Ok(Resolved {
                spec,
                args: args[len..].to_vec(),
            });
        }
        return Err(ResolveError::Unknown(args.join(" ")));
    }

    Err(ResolveError::NoCommand)
}

fn basename(argv0: &str) -> String {
    Path::new(argv0)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| argv0.to_string())
}

/// The `name<TAB>command` lines `--symlinks` prints and `check-system.sh`
/// reads: only the links the installer creates today.
pub fn installed_symlinks() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for spec in COMMANDS {
        for name in spec.installed {
            out.push((*name, spec.path));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn dispatch_resolves_symlink_names() {
        // The exact pair spec 001 names: the old script name and the new one.
        let by_symlink = resolve("rofi-files", &[]).unwrap();
        let by_words = resolve("lazyos", &args(&["menu", "files"])).unwrap();
        assert_eq!(by_symlink.spec.path, by_words.spec.path);
        assert_eq!(by_symlink.spec.path, "menu files");
        assert_eq!(
            resolve("/usr/bin/lazyos-menu", &[]).unwrap().spec.path,
            "menu"
        );
    }

    #[test]
    fn dispatch_takes_the_longest_match_and_keeps_the_rest() {
        let r = resolve("lazyos", &args(&["menu", "files", "Pictures"])).unwrap();
        assert_eq!(r.spec.path, "menu files");
        assert_eq!(r.args, args(&["Pictures"]));
        assert_eq!(
            resolve("lazyos", &args(&["menu", "bogus"])).unwrap().args,
            args(&["bogus"])
        );
    }

    #[test]
    fn dispatch_rejects_unknown() {
        assert_eq!(
            resolve("lazyos", &args(&["nope"])),
            Err(ResolveError::Unknown("nope".into()))
        );
        assert_eq!(resolve("lazyos", &[]), Err(ResolveError::NoCommand));
        // A symlink name that is not in the table is not a command either.
        assert_eq!(
            resolve("unknown-tool", &args(&["menu"])),
            Ok(Resolved {
                spec: &COMMANDS[0],
                args: vec![]
            })
        );
    }

    #[test]
    fn the_table_has_no_duplicate_names() {
        let mut seen = std::collections::BTreeSet::new();
        for spec in COMMANDS {
            assert!(
                seen.insert(spec.path),
                "duplicate command path {}",
                spec.path
            );
            for name in spec.symlinks {
                assert!(seen.insert(*name), "duplicate name {name}");
            }
            assert!(
                spec.installed.iter().all(|n| spec.symlinks.contains(n)),
                "{} installs a name it does not declare",
                spec.path
            );
        }
    }

    #[test]
    fn every_installed_symlink_names_a_command_help_lists() {
        for (name, path) in installed_symlinks() {
            assert!(!name.is_empty());
            assert!(
                COMMANDS.iter().any(|c| c.path == path),
                "{name} points at an unknown command {path}"
            );
        }
    }

    #[test]
    fn shapes_carry_their_own_app_id_and_size() {
        assert_eq!(Shape::Pick.app_id(), "lazyos-pick");
        assert_eq!(Shape::Task.app_id(), "lazyos-task");
        assert_eq!(Shape::Full.app_id(), "lazyos-full");
        // The size is what the sway drop-in repeats for the same app-id; a
        // shape without one owns the whole screen.
        assert_eq!(Shape::Task.size(), Some((940, 620)));
        assert_eq!(Shape::Full.size(), None);
    }

    #[test]
    fn the_child_is_given_the_canonical_words_not_the_symlink() {
        let r = resolve("rofi-files", &args(&["--search"])).unwrap();
        assert_eq!(r.words(), vec!["menu", "files"]);
        assert_eq!(r.args, args(&["--search"]));
    }
}
