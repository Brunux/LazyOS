//! Where a surface is generated, and how it is written without surprises.
//!
//! Three rules, and each has a failure it prevents:
//!
//! - **Atomic**: the new content goes to `<target>.new` in the *same directory*
//!   and is renamed over the target, so an interrupted `theme set` leaves the
//!   old file whole. A rename across filesystems is not atomic, which is why the
//!   temp file is a sibling and not `/tmp`.
//! - **Idempotent**: the last generated body is kept in the state directory; if
//!   the target already holds exactly that, nothing is written and the mtime
//!   does not move. A second `theme set` is a no-op, byte for byte.
//! - **A hand edit is not overwritten in silence**: if the target is not what
//!   the engine last wrote, the difference is shown and the write waits for
//!   `--force`. A target with no generated header at all is never ours, so it
//!   waits too.

use std::fs;
use std::path::{Path, PathBuf};

use crate::theme::Theme;

/// One generated file: the embedded template and where it lands, relative to
/// `$HOME`. The order is the order `theme set` writes them, which is the order a
/// failure is reported in.
pub struct Surface {
    pub name: &'static str,
    pub template: &'static str,
    pub target: &'static str,
}

/// The session's surfaces. A new one is one line here and one template file -
/// and a golden test, because the test renders whatever is in this table.
pub const SURFACES: [Surface; 12] = [
    Surface {
        name: "foot",
        template: include_str!("../templates/foot.ini.tpl"),
        target: ".config/foot/foot.ini",
    },
    Surface {
        name: "waybar",
        template: include_str!("../templates/waybar-style.css.tpl"),
        target: ".config/waybar/style.css",
    },
    Surface {
        name: "mako",
        template: include_str!("../templates/mako-config.tpl"),
        target: ".config/mako/config",
    },
    Surface {
        name: "rofi",
        template: include_str!("../templates/rofi-solarized.rasi.tpl"),
        target: ".config/rofi/solarized.rasi",
    },
    Surface {
        name: "starship",
        template: include_str!("../templates/starship.toml.tpl"),
        target: ".config/starship.toml",
    },
    Surface {
        name: "btop",
        template: include_str!("../templates/btop.theme.tpl"),
        target: ".config/btop/themes/lazyos.theme",
    },
    Surface {
        name: "lazygit",
        template: include_str!("../templates/lazygit.yml.tpl"),
        target: ".config/lazygit/config.yml",
    },
    Surface {
        name: "nvim",
        template: include_str!("../templates/nvim.vim.tpl"),
        target: ".config/nvim/colors/lazyos.vim",
    },
    Surface {
        name: "fzf",
        template: include_str!("../templates/fzf.env.tpl"),
        target: ".config/lazyos/fzf.env",
    },
    Surface {
        name: "swaylock",
        template: include_str!("../templates/swaylock.tpl"),
        target: ".config/swaylock/config",
    },
    Surface {
        name: "sway",
        template: include_str!("../templates/sway-theme.conf.tpl"),
        target: ".config/sway/config.d/35-theme.conf",
    },
    Surface {
        name: "greeter",
        template: include_str!("../templates/greeter.tpl"),
        target: ".config/lazyos/greeter",
    },
];

/// What one surface did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The file already held exactly this - nothing was written.
    Unchanged,
    /// Written (first time, a theme switch, or `--force` over a hand edit).
    Written,
    /// The file is not what the engine last wrote and `--force` was not given.
    Edited,
}

/// The paths the engine owns under one home directory. Tests build them from a
/// temp directory; the CLI builds them from `$HOME` and `$XDG_*`.
pub struct Paths {
    pub home: PathBuf,
    pub state: PathBuf,
}

impl Paths {
    pub fn new(home: impl Into<PathBuf>, state: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            state: state.into(),
        }
    }

    pub fn target(&self, surface: &Surface) -> PathBuf {
        self.home.join(surface.target)
    }

    /// The last body the engine generated for this surface. Its presence is
    /// what distinguishes "the template moved on" from "a person edited this".
    pub fn record(&self, surface: &Surface) -> PathBuf {
        self.state.join("generated").join(surface.name)
    }

    /// The active theme slug, one line. `theme current` is the only reader.
    pub fn active(&self) -> PathBuf {
        self.state.join("theme")
    }
}

/// Writes `body` to `target` atomically, creating its directory. The temp file
/// is a sibling so the rename cannot cross a filesystem.
pub fn write_atomic(target: &Path, body: &str) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| format!("{} has no directory", target.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let temp = sibling(target, "new");
    fs::write(&temp, body).map_err(|error| format!("cannot write {}: {error}", temp.display()))?;
    fs::rename(&temp, target).map_err(|error| {
        let _ = fs::remove_file(&temp);
        format!("cannot replace {}: {error}", target.display())
    })
}

fn sibling(target: &Path, extension: &str) -> PathBuf {
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(extension);
    target.with_file_name(name)
}

/// Generates one surface from the embedded template.
pub fn render_surface(
    surface: &Surface,
    theme: &Theme,
    slug: &str,
    home: &str,
) -> Result<String, String> {
    crate::render::render(
        surface.template,
        &crate::render::Context { theme, slug, home },
    )
    .map_err(|error| format!("{}: {error}", surface.name))
}

/// Writes one surface, honouring the three rules above.
pub fn install(
    paths: &Paths,
    surface: &Surface,
    theme: &Theme,
    slug: &str,
    force: bool,
) -> Result<Outcome, String> {
    let body = render_surface(surface, theme, slug, &paths.home.to_string_lossy())?;
    let target = paths.target(surface);
    let record = paths.record(surface);

    let current = fs::read_to_string(&target).ok();
    let generated = fs::read_to_string(&record).ok();

    let write = match &current {
        None => true,
        Some(text) if *text == body => {
            // Nothing to write - but a card that predates the record still has
            // to leave one, or the next run mistakes it for a hand edit.
            if generated.as_deref() != Some(text.as_str()) {
                write_record(&record, &body)?;
            }
            return Ok(Outcome::Unchanged);
        }
        Some(text) => match &generated {
            // Exactly what we wrote last time, so the template moved on (an
            // image upgrade): regenerate.
            Some(last) if last == text => true,
            // Ours, and changed since: a person edited it.
            Some(_) => force,
            // No record at all: ours only if it carries the header.
            None => force || is_ours(text),
        },
    };
    if !write {
        return Ok(Outcome::Edited);
    }
    write_atomic(&target, &body)?;
    write_record(&record, &body)?;
    Ok(Outcome::Written)
}

fn write_record(record: &Path, body: &str) -> Result<(), String> {
    if let Some(parent) = record.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::write(record, body).map_err(|error| format!("cannot record {}: {error}", record.display()))
}

/// A file this engine generated carries the header; anything else is somebody
/// else's file and is not overwritten without `--force`.
fn is_ours(text: &str) -> bool {
    text.lines()
        .take(2)
        .any(|line| line.contains("LazyOS theme:"))
}

/// A short, readable account of how two bodies differ: the first differing
/// lines and the count, because a 190-line stylesheet must not be printed to
/// answer "did anything change".
pub fn diff_summary(old: &str, new: &str) -> String {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let mut out = String::new();
    let mut differing = 0;
    for index in 0..old.len().max(new.len()) {
        let before = old.get(index).copied().unwrap_or("");
        let after = new.get(index).copied().unwrap_or("");
        if before != after {
            differing += 1;
            if differing <= 6 {
                out.push_str(&format!(
                    "  line {}: -{before}\n            +{after}\n",
                    index + 1
                ));
            }
        }
    }
    if differing > 6 {
        out.push_str(&format!("  ... and {} more lines\n", differing - 6));
    }
    out
}

/// The active theme: one line, written atomically like everything else.
pub fn set_active(paths: &Paths, slug: &str) -> Result<(), String> {
    write_atomic(&paths.active(), &format!("{slug}\n"))
}

pub fn active(paths: &Paths) -> Option<String> {
    let text = fs::read_to_string(paths.active()).ok()?;
    let slug = text.lines().next()?.trim().to_string();
    if slug.is_empty() { None } else { Some(slug) }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Temp(PathBuf);

    impl Temp {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "lazyos-theme-test-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn paths(&self) -> Paths {
            Paths::new(self.0.join("home"), self.0.join("state"))
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn solarized() -> Theme {
        Theme::parse(crate::builtin::SOLARIZED).unwrap()
    }

    fn surface(name: &str) -> &'static Surface {
        SURFACES.iter().find(|s| s.name == name).unwrap()
    }

    #[test]
    fn the_first_write_lands_and_the_second_is_unchanged() {
        let temp = Temp::new("idempotent");
        let paths = temp.paths();
        let foot = surface("foot");
        assert_eq!(
            install(&paths, foot, &solarized(), "solarized", false).unwrap(),
            Outcome::Written
        );
        let target = paths.target(foot);
        let first = fs::read_to_string(&target).unwrap();
        let mtime = fs::metadata(&target).unwrap().modified().unwrap();
        assert_eq!(
            install(&paths, foot, &solarized(), "solarized", false).unwrap(),
            Outcome::Unchanged
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), first);
        assert_eq!(
            fs::metadata(&target).unwrap().modified().unwrap(),
            mtime,
            "an unchanged surface must not be rewritten (a running foot watches the file)"
        );
    }

    #[test]
    fn a_theme_change_moves_the_colours_and_the_header() {
        let temp = Temp::new("switch");
        let paths = temp.paths();
        let mako = surface("mako");
        install(&paths, mako, &solarized(), "solarized", false).unwrap();
        let solarized = fs::read_to_string(paths.target(mako)).unwrap();
        let mocha = Theme::parse(crate::builtin::CATPPUCCIN).unwrap();
        install(&paths, mako, &mocha, "catppuccin", false).unwrap();
        let switched = fs::read_to_string(paths.target(mako)).unwrap();
        assert_ne!(solarized, switched);
        assert!(
            switched.contains("#1e1e2e"),
            "the new background is written"
        );
        assert!(
            switched.contains("Catppuccin Mocha (catppuccin)"),
            "the header names it"
        );
    }

    #[test]
    fn a_hand_edit_is_refused_until_force_and_then_replaced() {
        let temp = Temp::new("edited");
        let paths = temp.paths();
        let mako = surface("mako");
        install(&paths, mako, &solarized(), "solarized", false).unwrap();
        let target = paths.target(mako);
        let mut text = fs::read_to_string(&target).unwrap();
        text.push_str("\n# mine\n");
        fs::write(&target, &text).unwrap();

        assert_eq!(
            install(&paths, mako, &solarized(), "solarized", false).unwrap(),
            Outcome::Edited
        );
        assert!(
            fs::read_to_string(&target).unwrap().contains("# mine"),
            "left alone"
        );

        assert_eq!(
            install(&paths, mako, &solarized(), "solarized", true).unwrap(),
            Outcome::Written
        );
        assert!(!fs::read_to_string(&target).unwrap().contains("# mine"));
    }

    #[test]
    fn a_file_the_engine_never_generated_is_not_overwritten_in_silence() {
        let temp = Temp::new("foreign");
        let paths = temp.paths();
        let foot = surface("foot");
        let target = paths.target(foot);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "[colors-dark]\nforeground=112233\n").unwrap();
        assert_eq!(
            install(&paths, foot, &solarized(), "solarized", false).unwrap(),
            Outcome::Edited
        );
        assert!(fs::read_to_string(&target).unwrap().contains("112233"));
    }

    #[test]
    fn a_failed_write_leaves_the_old_file_whole_and_no_temp_behind() {
        let temp = Temp::new("atomic");
        let paths = temp.paths();
        let foot = surface("foot");
        install(&paths, foot, &solarized(), "solarized", false).unwrap();
        let target = paths.target(foot);
        let before = fs::read_to_string(&target).unwrap();

        // A directory in place of the temp file is the cheapest way to make the
        // write fail halfway: `write` to it cannot succeed.
        let temp_path = sibling(&target, "new");
        fs::create_dir_all(&temp_path).unwrap();
        let error = write_atomic(&target, "new body").unwrap_err();
        assert!(
            error.contains("can not") || error.contains("cannot"),
            "{error}"
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            before,
            "the old file survived"
        );
        fs::remove_dir_all(&temp_path).unwrap();
        assert!(!sibling(&target, "new").exists());
    }

    #[test]
    fn the_active_theme_is_one_line_that_round_trips() {
        let temp = Temp::new("active");
        let paths = temp.paths();
        assert_eq!(active(&paths), None);
        set_active(&paths, "gruvbox").unwrap();
        assert_eq!(active(&paths).as_deref(), Some("gruvbox"));
        assert_eq!(fs::read_to_string(paths.active()).unwrap(), "gruvbox\n");
    }

    #[test]
    fn a_diff_summary_shows_the_change_and_counts_the_rest() {
        let summary = diff_summary("a\nb\nc\n", "a\nB\nc\n");
        assert!(summary.contains("line 2"), "{summary}");
        let long_old: String = (0..20).map(|i| format!("{i}\n")).collect();
        let long_new: String = (0..20)
            .map(|i| {
                if i < 10 {
                    format!("{i}\n")
                } else {
                    format!("changed {i}\n")
                }
            })
            .collect();
        let summary = diff_summary(&long_old, &long_new);
        assert!(summary.contains("4 more"), "{summary}");
    }
}
