//! The tool table: what the agent can ask for, at which tier, and which program
//! answers.
//!
//! This file is the security boundary of spec 008, so it is data and nothing
//! else. `tools/list` is generated from [`TOOLS`], `lazyos mcp --tools` prints
//! the same rows, and the manifest version is a hash of them: the three cannot
//! disagree, because there is one table.
//!
//! The tiers are the plan's (§13.2):
//!
//! - **read** changes nothing and returns text, JSON or a path;
//! - **act** is visual and reversible - a window opens, a theme changes;
//! - **propose** opens the real, already-gated window a person uses (Cancel
//!   first, a typed disk name, a second answer) and returns; it executes
//!   nothing, and the gate is never visible to the agent;
//! - **refuse** does not exist. [`REFUSED`] lists the names so a call to one is
//!   answered with the menu to point the person at - and so a `const` assertion
//!   below can make it impossible to put one in [`TOOLS`] by mistake.

/// A tool's permission tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Read,
    Act,
    Propose,
    /// Never served. The variant exists so the table's type could hold one and
    /// the build-time assertion below is what keeps it out.
    Refuse,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Read => "read",
            Tier::Act => "act",
            Tier::Propose => "propose",
            Tier::Refuse => "refuse",
        }
    }
}

/// What answers a tool: always a program and its literal words, never a shell
/// string. The binary implements each variant with `Command::new` on a fixed
/// program; `lazyos-cli/tests/mcp.rs` resolves every one of them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Maps {
    /// This binary with these words (`lazyos theme set`). The first words are a
    /// command of the table; what follows is flags it takes.
    Lazyos(&'static [&'static str]),
    /// A session script in `~/.config/bin`, the one the menu row runs.
    Session(&'static str),
    /// A program on `PATH`, read by the server (a read tool's data source).
    Program(&'static str),
    /// Answered in the server's own process from procfs/sysfs/state files.
    Internal,
}

impl Maps {
    /// The words `--tools` prints in its last column.
    pub fn describe(self) -> String {
        match self {
            Maps::Lazyos(words) => format!("lazyos {}", words.join(" ")),
            Maps::Session(name) => format!("~/.config/bin/{name}"),
            Maps::Program(name) => name.to_string(),
            Maps::Internal => "internal".to_string(),
        }
    }
}

/// How often a tool may run. Enforced by the server before the handler sees the
/// call, so a limit is a property of the table, printed by `--tools`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    None,
    /// At most this many calls in any 60 seconds (the plan's notify cap).
    PerMinute(u32),
    /// Seconds before the same tool may run again: a propose tool opens a
    /// window, and an agent in a loop must not stack confirms on the screen.
    Cooldown(u32),
}

impl Limit {
    pub fn describe(self) -> String {
        match self {
            Limit::None => "-".to_string(),
            Limit::PerMinute(n) => format!("{n}/min"),
            Limit::Cooldown(s) => format!("1/{s}s"),
        }
    }
}

/// One tool.
#[derive(Debug)]
pub struct ToolSpec {
    pub name: &'static str,
    pub tier: Tier,
    pub title: &'static str,
    /// What the model reads; it says what the person sees.
    pub description: &'static str,
    /// The input schema, JSON. Parsed at startup; a test parses every one.
    pub schema: &'static str,
    pub maps: Maps,
    pub limit: Limit,
}

/// A name that is never served, and where the person does it instead.
#[derive(Debug)]
pub struct Refused {
    /// An exact name, or a prefix ending in `*` (`profile_*`).
    pub name: &'static str,
    pub instead: &'static str,
}

impl Refused {
    pub fn matches(&self, name: &str) -> bool {
        match self.name.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => name == self.name,
        }
    }
}

const NO_ARGS: &str = r#"{"type":"object","properties":{},"additionalProperties":false}"#;

const fn tool(
    name: &'static str,
    tier: Tier,
    title: &'static str,
    description: &'static str,
    schema: &'static str,
    maps: Maps,
    limit: Limit,
) -> ToolSpec {
    ToolSpec {
        name,
        tier,
        title,
        description,
        schema,
        maps,
        limit,
    }
}

use Limit::{Cooldown, PerMinute};
use Tier::{Act, Propose, Read};

/// The served tools, in the order `tools/list` returns them.
pub const TOOLS: &[ToolSpec] = &[
    // -- read: no change, data back ------------------------------------------
    tool(
        "desktop_status",
        Read,
        "Desktop status",
        "The machine at a glance: host, uptime, load, memory and swap, battery, disk space, \
         the active theme and the LazyOS version. Changes nothing.",
        NO_ARGS,
        Maps::Internal,
        Limit::None,
    ),
    tool(
        "desktop_screenshot",
        Read,
        "Screenshot",
        "Capture the screen (or one output) to a PNG and return its path. The file is \
         private (0600, under $XDG_RUNTIME_DIR), at most four are kept, and all of them are \
         deleted when this session ends. What is on the screen is what you see, including \
         anything private the person has open.",
        r#"{"type":"object","properties":{"output":{"type":"string","minLength":1,"maxLength":32,"description":"A sway output name (eDP-1, DP-1); the whole screen when omitted"}},"additionalProperties":false}"#,
        Maps::Program("grim"),
        PerMinute(12),
    ),
    tool(
        "desktop_windows",
        Read,
        "Windows",
        "The open windows: workspace, app id, title, which one is focused, which float.",
        NO_ARGS,
        Maps::Program("swaymsg"),
        Limit::None,
    ),
    tool(
        "theme_current",
        Read,
        "Current theme",
        "The active palette: its slug and its name.",
        NO_ARGS,
        Maps::Lazyos(&["theme", "current"]),
        Limit::None,
    ),
    tool(
        "packages_installed",
        Read,
        "Installed packages",
        "Installed Debian packages and their versions, optionally only those whose name \
         contains a word.",
        r#"{"type":"object","properties":{"filter":{"type":"string","maxLength":64},"limit":{"type":"integer","minimum":1,"maximum":2000}},"additionalProperties":false}"#,
        Maps::Program("dpkg-query"),
        Limit::None,
    ),
    tool(
        "system_report",
        Read,
        "System report",
        "Run check-system.sh, the machine's own health report, and return its counts and \
         every FAIL and WARN line. Takes up to a minute; changes nothing.",
        NO_ARGS,
        Maps::Program("check-system.sh"),
        Cooldown(60),
    ),
    tool(
        "clipboard_read",
        Read,
        "Read the clipboard",
        "The text on the clipboard (up to 64 KiB). It may hold something the person copied \
         on purpose and did not mean to share; quote it only when asked.",
        NO_ARGS,
        Maps::Program("wl-paste"),
        Limit::None,
    ),
    tool(
        "task_list",
        Read,
        "Tasks",
        "The recent slow actions (scans, installs, downloads, questions): id, title, state, \
         whether it can be stopped, and its result.",
        NO_ARGS,
        Maps::Internal,
        Limit::None,
    ),
    // -- act: visual, reversible ---------------------------------------------
    tool(
        "menu_open",
        Act,
        "Open the menu",
        "Open the root menu (or the command palette) in front of the person, optionally with \
         a filter already typed, so the row you mean is the first one. The person picks.",
        r#"{"type":"object","properties":{"surface":{"type":"string","enum":["root","palette"]},"filter":{"type":"string","maxLength":48}},"additionalProperties":false}"#,
        Maps::Lazyos(&["menu"]),
        Limit::None,
    ),
    tool(
        "task_open",
        Act,
        "Show a task",
        "Open the window of a task by id (from task_list): its checklist, its log and its \
         result.",
        r#"{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":96}},"required":["id"],"additionalProperties":false}"#,
        Maps::Lazyos(&["task", "attach"]),
        Limit::None,
    ),
    tool(
        "task_stop",
        Act,
        "Stop a task",
        "Ask a running task to stop - only one whose record says it is stoppable (an install \
         or a format never is).",
        r#"{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":96}},"required":["id"],"additionalProperties":false}"#,
        Maps::Lazyos(&["task", "stop"]),
        Limit::None,
    ),
    tool(
        "theme_set",
        Act,
        "Set the theme",
        "Re-theme the desktop with a built-in or installed theme (theme_current names the \
         active one). Every surface is regenerated and the session reloads.",
        r#"{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":40}},"required":["name"],"additionalProperties":false}"#,
        Maps::Lazyos(&["theme", "set"]),
        PerMinute(4),
    ),
    tool(
        "notify_send",
        Act,
        "Send a card",
        "Show a notification card with a title, a body and a picture. At most three a \
         minute: prefer opening the window that holds the answer over a toast that expires.",
        r#"{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":80},"body":{"type":"string","maxLength":400},"icon":{"type":"string","maxLength":40},"urgency":{"type":"string","enum":["low","normal","critical"]}},"required":["title"],"additionalProperties":false}"#,
        Maps::Lazyos(&["notify", "send"]),
        PerMinute(3),
    ),
    tool(
        "view_open",
        Act,
        "Open a file",
        "Open a file or a directory the way a double click does: a document in its reader, \
         a picture in the viewer, text in bat, a directory in the file list.",
        r#"{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096}},"required":["path"],"additionalProperties":false}"#,
        Maps::Session("lazyos-view"),
        PerMinute(10),
    ),
    // -- propose: the real gated window, the person decides ------------------
    tool(
        "install_propose",
        Propose,
        "Propose installing LazyOS",
        "Open the Install LazyOS window, which checks the machine and asks the person to type \
         the disk's name. Nothing is written unless they do; you will not see their answer.",
        NO_ARGS,
        Maps::Session("rofi-install"),
        Cooldown(30),
    ),
    tool(
        "format_propose",
        Propose,
        "Propose formatting a disk",
        "Open the Format a disk menu: it lists only removable disks this system does not use, \
         and asks once more with Cancel first. The person picks the disk and confirms.",
        NO_ARGS,
        Maps::Session("rofi-format"),
        Cooldown(30),
    ),
    tool(
        "swap_propose",
        Propose,
        "Propose auxiliary swap",
        "Open the Auxiliary swap menu, which turns a card into extra memory after the person \
         picks the disk, the size, and confirms with Cancel first.",
        NO_ARGS,
        Maps::Session("rofi-swap"),
        Cooldown(30),
    ),
    tool(
        "power_propose",
        Propose,
        "Propose ending the session",
        "Open the Power confirm for log out, reboot or shut down, with Cancel selected. The \
         person's second answer is the only thing that ends the session.",
        r#"{"type":"object","properties":{"action":{"type":"string","enum":["logout","reboot","poweroff"]}},"required":["action"],"additionalProperties":false}"#,
        Maps::Lazyos(&["menu", "--power"]),
        Cooldown(30),
    ),
];

/// The names that are never served (spec 008 §Scope), with the menu a person
/// uses instead. A call to one is refused with that sentence and logged.
pub const REFUSED: &[Refused] = &[
    Refused {
        name: "run",
        instead: "there is no second exec surface: your own shell session is your command line",
    },
    Refused {
        name: "install_apply",
        instead: "install_propose opens Install LazyOS; the person types the disk's name",
    },
    Refused {
        name: "format_apply",
        instead: "format_propose opens Places > Format a disk; the person picks and confirms",
    },
    Refused {
        name: "swap_apply",
        instead: "swap_propose opens Places > Auxiliary swap; the person picks and confirms",
    },
    Refused {
        name: "power_apply",
        instead: "power_propose opens the Power confirm; the person's second answer ends the session",
    },
    Refused {
        name: "kpart_write",
        instead: "the kernel partition is written by scripts/install-kpart.sh with CONFIRM=yes, by the person",
    },
    Refused {
        name: "partition_write",
        instead: "partitions are written only by the disk menus and the install scripts",
    },
    Refused {
        name: "profile_*",
        instead: "Developer (Alt+Ctrl+D) exports and restores a profile; the passphrase is the person's",
    },
];

/// Whether `name` is refused, and why.
pub fn refused(name: &str) -> Option<&'static Refused> {
    REFUSED.iter().find(|r| r.matches(name))
}

pub fn find(name: &str) -> Option<&'static ToolSpec> {
    TOOLS.iter().find(|t| t.name == name)
}

// ---------------------------------------------------------------------------
// The build-time half of the boundary
// ---------------------------------------------------------------------------

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn refused_matches(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.as_bytes(), name.as_bytes());
    if !p.is_empty() && p[p.len() - 1] == b'*' {
        let prefix = p.len() - 1;
        if n.len() < prefix {
            return false;
        }
        let mut i = 0;
        while i < prefix {
            if p[i] != n[i] {
                return false;
            }
            i += 1;
        }
        return true;
    }
    str_eq(pattern, name)
}

/// The table may not hold a refuse-tier tool, a refused name, a duplicate, or a
/// name a model provider would reject. This runs when the crate is compiled:
/// a row that breaks it is a build error, not a review comment.
const _: () = {
    let mut i = 0;
    while i < TOOLS.len() {
        let tool = &TOOLS[i];
        assert!(
            !matches!(tool.tier, Tier::Refuse),
            "a refuse-tier tool is in TOOLS"
        );
        let mut j = 0;
        while j < REFUSED.len() {
            assert!(
                !refused_matches(REFUSED[j].name, tool.name),
                "a refused name is in TOOLS"
            );
            j += 1;
        }
        let mut k = i + 1;
        while k < TOOLS.len() {
            assert!(
                !str_eq(tool.name, TOOLS[k].name),
                "a tool name is listed twice"
            );
            k += 1;
        }
        let bytes = tool.name.as_bytes();
        assert!(
            !bytes.is_empty() && bytes.len() <= 64,
            "a tool name is empty or too long"
        );
        let mut b = 0;
        while b < bytes.len() {
            let c = bytes[b];
            assert!(
                c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_',
                "a tool name has a character outside [a-z0-9_]"
            );
            b += 1;
        }
        i += 1;
    }
};

// ---------------------------------------------------------------------------
// The manifest: what --tools prints and what the instructions pin
// ---------------------------------------------------------------------------

/// FNV-1a, 64 bits: a stable fingerprint, not a secret. The manifest version
/// only has to change whenever a row does.
pub fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// The schema re-serialised (`serde_json` sorts object keys here), so the hash
/// does not depend on how the literal above is spaced.
pub fn canonical_schema(tool: &ToolSpec) -> serde_json::Value {
    serde_json::from_str(tool.schema)
        .unwrap_or_else(|error| panic!("{}: the input schema is not JSON: {error}", tool.name))
}

pub fn schema_hash(tool: &ToolSpec) -> String {
    let canonical = canonical_schema(tool).to_string();
    format!("{:016x}", fingerprint(canonical.as_bytes()))
}

/// One `--tools` row: `tier<TAB>name<TAB>schema hash<TAB>limit<TAB>maps to`.
pub fn row(tool: &ToolSpec) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}",
        tool.tier.label(),
        tool.name,
        schema_hash(tool),
        tool.limit.describe(),
        tool.maps.describe()
    )
}

/// The manifest version: a hash of every row, so any change to a name, a
/// tier, a schema, a limit or a mapping is a new version the instructions file
/// has to be updated to.
pub fn version() -> String {
    let mut all = String::new();
    for tool in TOOLS {
        all.push_str(&row(tool));
        all.push('\n');
    }
    format!("m-{:016x}", fingerprint(all.as_bytes()))
}

/// What `lazyos mcp --tools` prints: a header line with the version, then one
/// row per served tool. Refused names are not in it, by construction.
pub fn text() -> String {
    let mut out = format!(
        "# lazyos-mcp manifest {} ({} tools; tier, name, schema, limit, maps to)\n",
        version(),
        TOOLS.len()
    );
    for tool in TOOLS {
        out.push_str(&row(tool));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuse_never_listed() {
        for tool in TOOLS {
            assert_ne!(tool.tier, Tier::Refuse, "{}", tool.name);
            assert!(refused(tool.name).is_none(), "{} is refused", tool.name);
        }
        // And the printed manifest, which is what check-system.sh reads.
        for line in text().lines().skip(1) {
            let mut fields = line.split('\t');
            let tier = fields.next().unwrap();
            let name = fields.next().unwrap();
            assert_ne!(tier, "refuse", "{line}");
            assert!(refused(name).is_none(), "--tools lists the refused {name}");
        }
    }

    #[test]
    fn the_refused_names_are_the_specs() {
        for name in [
            "run",
            "format_apply",
            "install_apply",
            "swap_apply",
            "power_apply",
            "kpart_write",
            "partition_write",
            "profile_export",
            "profile_restore",
        ] {
            assert!(refused(name).is_some(), "{name} must be refused");
        }
        assert!(
            refused("profile").is_none(),
            "a prefix pattern needs its underscore"
        );
        assert!(refused("desktop_status").is_none());
    }

    #[test]
    fn every_schema_is_an_object_that_refuses_unknown_arguments() {
        for tool in TOOLS {
            let schema = canonical_schema(tool);
            assert_eq!(schema["type"], "object", "{}", tool.name);
            assert_eq!(
                schema["additionalProperties"],
                serde_json::Value::Bool(false),
                "{} must refuse an argument it does not declare",
                tool.name
            );
            for required in schema["required"].as_array().into_iter().flatten() {
                let name = required.as_str().unwrap();
                assert!(
                    schema["properties"].get(name).is_some(),
                    "{}: required {name} is not a property",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn the_tiers_are_where_the_spec_puts_them() {
        let tier = |name: &str| find(name).unwrap_or_else(|| panic!("{name} missing")).tier;
        for name in [
            "desktop_status",
            "desktop_screenshot",
            "desktop_windows",
            "theme_current",
            "packages_installed",
            "system_report",
            "clipboard_read",
            "task_list",
        ] {
            assert_eq!(tier(name), Tier::Read, "{name}");
        }
        for name in [
            "menu_open",
            "task_open",
            "task_stop",
            "theme_set",
            "notify_send",
            "view_open",
        ] {
            assert_eq!(tier(name), Tier::Act, "{name}");
        }
        for name in [
            "install_propose",
            "format_propose",
            "swap_propose",
            "power_propose",
        ] {
            assert_eq!(tier(name), Tier::Propose, "{name}");
            assert!(
                matches!(find(name).unwrap().limit, Limit::Cooldown(_)),
                "{name} opens a window and must have a cooldown"
            );
        }
        assert_eq!(TOOLS.len(), 18);
    }

    #[test]
    fn no_tool_maps_to_a_shell() {
        for tool in TOOLS {
            let words: Vec<&str> = match tool.maps {
                Maps::Lazyos(words) => words.to_vec(),
                Maps::Session(name) | Maps::Program(name) => vec![name],
                Maps::Internal => vec![],
            };
            for word in words {
                assert!(
                    !matches!(word, "sh" | "bash" | "dash" | "-c" | "env" | "sudo"),
                    "{} maps to {word}",
                    tool.name
                );
                assert!(
                    !word.contains([' ', ';', '|', '&', '$', '`', '<', '>']),
                    "{}: {word:?}",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn the_version_moves_with_any_row() {
        let before = version();
        assert!(before.starts_with("m-") && before.len() == 18, "{before}");
        // A schema hash ignores spacing but not content.
        let a = ToolSpec {
            schema: r#"{"type":"object", "properties":{}}"#,
            ..tool_copy(&TOOLS[0])
        };
        let b = ToolSpec {
            schema: r#"{"properties":{},"type":"object"}"#,
            ..tool_copy(&TOOLS[0])
        };
        let c = ToolSpec {
            schema: r#"{"properties":{"x":{"type":"string"}},"type":"object"}"#,
            ..tool_copy(&TOOLS[0])
        };
        assert_eq!(schema_hash(&a), schema_hash(&b));
        assert_ne!(schema_hash(&a), schema_hash(&c));
        assert_eq!(
            version(),
            before,
            "the version is a pure function of the table"
        );
    }

    fn tool_copy(t: &ToolSpec) -> ToolSpec {
        ToolSpec {
            name: t.name,
            tier: t.tier,
            title: t.title,
            description: t.description,
            schema: t.schema,
            maps: t.maps,
            limit: t.limit,
        }
    }

    #[test]
    fn manifest_snapshot() {
        // The manifest is reviewed as text: a changed tier, a new tool or a new
        // mapping is a visible diff of tests/manifest.txt. Regenerate with
        // UPDATE_SNAPSHOTS=1 and read the diff.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/manifest.txt");
        let rendered = text();
        if std::env::var("UPDATE_SNAPSHOTS").as_deref() == Ok("1") {
            std::fs::write(&path, &rendered).unwrap();
            return;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!(
                "missing {}: UPDATE_SNAPSHOTS=1 cargo test -p lazyos-mcp",
                path.display()
            )
        });
        assert_eq!(
            rendered, expected,
            "the manifest changed; review tests/manifest.txt"
        );
    }
}
