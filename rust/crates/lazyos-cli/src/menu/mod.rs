//! `lazyos menu` and `lazyos menu palette` (spec 007).
//!
//! The root menu is `bar-menu` as data ([`table`]): the same rows, groups and
//! order, filtered while typing with the session's one ranking
//! ([`lazyos_ui::ListState::ranked`]) and boosted by what the person launches
//! ([`frecency`]). The palette is the same surface over every row the menu
//! has, plus the framework commands that need no question and the built-in
//! themes. A chosen row is an [`Action`], run after the window is gone.

pub mod frecency;
pub mod screen;
pub mod table;

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use lazyos_ui::{Card, Glyph, Row};

pub use table::{Action, Arg, Group, MenuItem, Program, SessionAction};

use frecency::Frecency;

/// Which list a menu window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Root,
    Palette,
}

/// A row at run time: a table row, or a palette row built from data (a theme).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub label: String,
    pub detail: String,
    pub glyph: Glyph,
    pub group: Group,
    pub action: Choice,
}

/// What an entry does: a table action, or setting one built-in theme (built
/// from the theme crate's list, so it is data and not a `&'static` slice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Table(Action),
    Theme(&'static str),
}

impl Entry {
    fn from_item(item: &MenuItem) -> Self {
        Self {
            key: item.key.to_string(),
            label: item.label.to_string(),
            detail: item.detail.to_string(),
            glyph: item.glyph,
            group: item.group,
            action: Choice::Table(item.action),
        }
    }

    pub fn row(&self, boost: u32) -> Row {
        Row::new(self.label.clone())
            .glyph(self.glyph)
            .detail(self.detail.clone())
            .value(self.group.label())
            .boost(boost)
    }
}

/// What the rows say that depends on this machine: which agent the AI rows
/// start, so a row tells the person what it will do before they choose it.
#[derive(Clone, Debug, Default)]
pub struct Context {
    /// The default agent's name, `●`-marked like the Agents menu marks it.
    pub agent: Option<String>,
    /// Why the Ask row's agent is not the default, when it is not.
    pub ask: Option<String>,
}

impl Context {
    pub fn from_vars(vars: &BTreeMap<String, String>) -> Self {
        let default = crate::ask::default_agent(vars).unwrap_or_else(|| "opencode".to_string());
        let ask = crate::ask::choose(vars).map(|choice| match choice.note {
            Some(note) => note,
            None => format!("{}, the answer here", choice.agent.name()),
        });
        Self {
            agent: Some(default),
            ask,
        }
    }
}

/// The rows of a surface, in the order they are drawn when nothing is typed.
pub fn entries(surface: Surface, context: &Context) -> Vec<Entry> {
    let mut entries: Vec<Entry> = table::ROOT.iter().map(Entry::from_item).collect();
    for entry in &mut entries {
        match entry.key.as_str() {
            "agent" => {
                if let Some(agent) = &context.agent {
                    entry.detail = format!("\u{25cf} {agent}, in ~/Work");
                }
            }
            "ask" => {
                if let Some(ask) = &context.ask {
                    entry.detail = ask.clone();
                }
            }
            _ => {}
        }
    }
    if surface == Surface::Palette {
        entries.extend(table::PALETTE_EXTRA.iter().map(Entry::from_item));
        for slug in lazyos_theme::builtin::slugs() {
            entries.push(Entry {
                key: format!("theme-{slug}"),
                label: format!("Theme: {}", title(slug)),
                detail: format!("lazyos theme set {slug}"),
                glyph: Glyph::Theme,
                group: Group::System,
                action: Choice::Theme(slug),
            });
        }
    }
    entries
}

/// `rose-pine` → `Rose Pine`.
fn title(slug: &str) -> String {
    slug.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The frecency file, beside the other state the framework keeps.
pub fn frecency_path(state: &Path) -> PathBuf {
    state.join("frecency.json")
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Where a session script lives: the same directory `bar-menu` ran from.
pub fn session_bin(vars: &BTreeMap<String, String>) -> PathBuf {
    let config = match vars.get("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(vars.get("HOME").map(String::as_str).unwrap_or("/")).join(".config"),
    };
    config.join("bin")
}

/// The argument vector a [`Choice`] runs, or `None` for one the menu does
/// itself (the password, the Ask strip, the power list, a session action).
pub fn argv(choice: Choice, vars: &BTreeMap<String, String>, exe: &str) -> Option<Vec<String>> {
    let home = vars.get("HOME").cloned().unwrap_or_else(|| "/".to_string());
    match choice {
        Choice::Theme(slug) => Some(vec![
            exe.to_string(),
            "theme".to_string(),
            "set".to_string(),
            slug.to_string(),
        ]),
        Choice::Table(Action::Run { program, args }) => {
            let mut argv = vec![match program {
                Program::Session(name) => session_bin(vars).join(name).display().to_string(),
                Program::System(name) => name.to_string(),
                Program::Lazyos => exe.to_string(),
            }];
            argv.extend(args.iter().map(|arg| match arg {
                Arg::Lit(text) => text.to_string(),
                Arg::Home => home.clone(),
            }));
            Some(argv)
        }
        Choice::Table(_) => None,
    }
}

/// Start `argv` detached from the window: a new session (`setsid`), no
/// terminal, the home directory as its working directory. The window closes
/// right after, and a SIGHUP from the closing `foot` must not reach the
/// program the person chose.
pub fn spawn_detached(argv: &[String], home: &str) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    let Some((program, args)) = argv.split_first() else {
        return Err(io::Error::other("an empty command"));
    };
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe, the only call between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    let _ = child.try_wait();
    Ok(())
}

/// What a session action runs, in order: the commands `rofi-power` runs, the
/// logind method first and `sudo -n` as the fallback for the two that need it.
pub fn session_attempts(action: SessionAction) -> Vec<Vec<&'static str>> {
    match action {
        SessionAction::Logout => vec![vec!["swaymsg", "exit"]],
        SessionAction::Reboot => vec![
            vec!["systemctl", "reboot"],
            vec!["sudo", "-n", "systemctl", "reboot"],
        ],
        SessionAction::Poweroff => vec![
            vec!["systemctl", "poweroff"],
            vec!["sudo", "-n", "systemctl", "poweroff"],
        ],
    }
}

/// Run a session action; a failure is a critical card, never silence.
pub fn run_session(action: SessionAction) -> bool {
    for attempt in session_attempts(action) {
        let ok = Command::new(attempt[0])
            .args(&attempt[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return true;
        }
    }
    let _ = Card::new("Power")
        .body(format!(
            "Could not {} ({} failed, with or without sudo)",
            action.verb().to_lowercase(),
            session_attempts(action)[0].join(" ")
        ))
        .urgency(lazyos_ui::Urgency::Critical)
        .send();
    false
}

/// 16 random bytes as 32 hex characters, from the kernel's generator: no `+`,
/// `/` or `=` to re-quote when the password is pasted (the rule `bar-menu`
/// followed with `openssl rand -hex 16`).
pub fn password() -> io::Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Remember a launch; a history that cannot be written is not worth a failure.
pub fn remember(state: &Path, key: &str) {
    let mut frecency = Frecency::load(frecency_path(state));
    frecency.record(key, now());
    let _ = frecency.save();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn vars() -> BTreeMap<String, String> {
        [("HOME", "/home/b"), ("PATH", "/usr/bin")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn menu_table_has_no_dupes() {
        for surface in [Surface::Root, Surface::Palette] {
            let mut keys = BTreeSet::new();
            let mut labels = BTreeSet::new();
            for entry in entries(surface, &Context::default()) {
                assert!(keys.insert(entry.key.clone()), "duplicate key {}", entry.key);
                assert!(
                    labels.insert(entry.label.clone()),
                    "duplicate label {}",
                    entry.label
                );
            }
        }
        for item in table::POWER {
            assert!(
                table::ROOT.iter().all(|root| root.key != item.key),
                "{} is both a root row and a power row",
                item.key
            );
        }
    }

    #[test]
    fn the_root_menu_keeps_every_bar_menu_row() {
        // The table comparison spec 007 asks for: no row `bar-menu` had may
        // go missing, and the order of those rows is the order it had.
        let keys: Vec<&str> = table::ROOT.iter().map(|i| i.key).collect();
        let kept: Vec<&str> = keys
            .iter()
            .copied()
            .filter(|k| table::BAR_MENU_KEYS.contains(k))
            .collect();
        assert_eq!(kept, table::BAR_MENU_KEYS, "bar-menu's rows, in its order");
        let added: Vec<&str> = keys
            .iter()
            .copied()
            .filter(|k| !table::BAR_MENU_KEYS.contains(k))
            .collect();
        assert_eq!(added, vec!["ask"], "the one row this spec adds");
    }

    #[test]
    fn the_groups_read_in_bar_menus_order() {
        let mut seen: Vec<Group> = Vec::new();
        for item in table::ROOT {
            if seen.last() != Some(&item.group) {
                assert!(
                    !seen.contains(&item.group),
                    "{} breaks its group {:?} in two",
                    item.key,
                    item.group
                );
                seen.push(item.group);
            }
        }
        assert_eq!(
            seen,
            vec![Group::Open, Group::Ai, Group::System, Group::Session]
        );
    }

    #[test]
    fn action_never_contains_a_shell_metacharacter() {
        // A row is a program and its words. Nothing in the table may look like
        // shell, because nothing in the table is ever given to one - and a row
        // that needed `;`, `|` or `$` would be a row that needs a shell.
        const META: &[char] = &[
            ';', '|', '&', '$', '`', '<', '>', '(', ')', '{', '}', '*', '?', '\\', '\n', '"', '\'',
        ];
        let all = table::ROOT
            .iter()
            .chain(table::POWER)
            .chain(table::PALETTE_EXTRA);
        for item in all {
            if let Action::Run { program, args } = item.action {
                let name = match program {
                    Program::Session(n) | Program::System(n) => n,
                    Program::Lazyos => "lazyos",
                };
                assert!(!name.contains(META) && !name.contains(' '), "{}: {name}", item.key);
                assert!(
                    !matches!(name, "sh" | "bash" | "dash" | "zsh"),
                    "{} runs a shell",
                    item.key
                );
                for arg in args {
                    if let Arg::Lit(text) = arg {
                        assert!(!text.contains(META), "{}: {text:?}", item.key);
                        assert_ne!(*text, "-c", "{} passes -c", item.key);
                    }
                }
            }
        }
        // And the argument vectors the actions become carry no shell either.
        for entry in entries(Surface::Palette, &Context::default()) {
            if let Some(argv) = argv(entry.action, &vars(), "/usr/bin/lazyos") {
                assert!(!argv.iter().any(|a| a == "-c"), "{}: {argv:?}", entry.key);
            }
        }
    }

    #[test]
    fn a_row_becomes_its_program_and_its_words() {
        let v = vars();
        let get = |key: &str| {
            entries(Surface::Palette, &Context::default())
                .into_iter()
                .find(|e| e.key == key)
                .unwrap()
                .action
        };
        assert_eq!(
            argv(get("files"), &v, "/usr/bin/lazyos").unwrap(),
            vec!["/home/b/.config/bin/rofi-files"]
        );
        assert_eq!(
            argv(get("files-window"), &v, "/usr/bin/lazyos").unwrap(),
            vec!["favnyr", "/home/b"]
        );
        assert_eq!(
            argv(get("agent"), &v, "/usr/bin/lazyos").unwrap(),
            vec!["/home/b/.config/bin/rofi-agents", "--default"]
        );
        assert_eq!(
            argv(get("theme-next"), &v, "/usr/bin/lazyos").unwrap(),
            vec!["/usr/bin/lazyos", "theme", "next"]
        );
        assert_eq!(
            argv(get("theme-rose-pine"), &v, "/usr/bin/lazyos").unwrap(),
            vec!["/usr/bin/lazyos", "theme", "set", "rose-pine"]
        );
        // The rows the menu does itself have no command line at all.
        for key in ["password", "ask", "power"] {
            assert_eq!(argv(get(key), &v, "/usr/bin/lazyos"), None, "{key}");
        }
    }

    #[test]
    fn the_session_actions_run_what_rofi_power_runs() {
        assert_eq!(
            session_attempts(SessionAction::Reboot),
            vec![
                vec!["systemctl", "reboot"],
                vec!["sudo", "-n", "systemctl", "reboot"]
            ]
        );
        assert_eq!(
            session_attempts(SessionAction::Logout),
            vec![vec!["swaymsg", "exit"]]
        );
    }

    #[test]
    fn the_rows_say_which_agent_they_start() {
        let context = Context {
            agent: Some("claude".to_string()),
            ask: Some("Claude Code, the answer here".to_string()),
        };
        let rows = entries(Surface::Root, &context);
        let detail = |key: &str| rows.iter().find(|e| e.key == key).unwrap().detail.clone();
        assert_eq!(detail("agent"), "\u{25cf} claude, in ~/Work");
        assert_eq!(detail("ask"), "Claude Code, the answer here");
    }

    #[test]
    fn the_password_is_32_hex_characters() {
        let a = password().unwrap();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, password().unwrap());
    }

    #[test]
    fn a_theme_slug_reads_as_a_name() {
        assert_eq!(title("rose-pine"), "Rose Pine");
        assert_eq!(title("solarized"), "Solarized");
    }
}
