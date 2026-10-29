//! The root menu as data (spec 007).
//!
//! Every row of the old `bar-menu` is a [`MenuItem`] here, in the same groups
//! and the same order, and every row's action is an [`Action`] value: a
//! program and an argument list, or one of the few things the menu does
//! itself. There is no shell anywhere in this file, which is the point - a row
//! cannot invent a command line, so a label, a detail or a future row from a
//! data file can never become `; rm -rf ~` (`action_never_contains_a_shell_metacharacter`).
//!
//! `check-system.sh` compares the keys below with the rows `bar-menu` had
//! (`lazyos menu --rows`), so a row that goes missing is a failed check, not a
//! documented keypress that answers nothing.

use lazyos_ui::Glyph;

/// The menu's groups, in the order they are drawn. `Open` is the unnamed first
/// block of `bar-menu` (what opens something); the others are its headings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Open,
    Ai,
    System,
    Session,
}

impl Group {
    /// The word the row shows in its value column, which is also something a
    /// person can type: `system` narrows the menu to the system group.
    pub fn label(self) -> &'static str {
        match self {
            Group::Open => "open",
            Group::Ai => "ai",
            Group::System => "system",
            Group::Session => "session",
        }
    }
}

/// One argument of a program a row runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arg {
    Lit(&'static str),
    /// The person's home directory, filled in when the row runs. It is a value
    /// and not `"$HOME"`, because nothing here expands a variable.
    Home,
}

/// Where a row's program comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    /// A session script, run from `~/.config/bin` by its full path - the same
    /// file `bar-menu` ran with `exec "$dir/…"`, so a script that is a symlink
    /// to `lazyos` today and a shell script tomorrow is the same row.
    Session(&'static str),
    /// A program from `PATH` (`foot`, `rofi`, `favnyr`).
    System(&'static str),
    /// This binary with these words (`lazyos theme next`), by the path the
    /// process was started from rather than by name.
    Lazyos,
}

/// The three actions that end the session. They are only reachable through a
/// [`lazyos_ui::Confirm`] whose first answer is Cancel (spec 007: a stray Enter
/// runs nothing), and they run the commands `rofi-power` runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionAction {
    Logout,
    Reboot,
    Poweroff,
}

impl SessionAction {
    pub fn question(self) -> &'static str {
        match self {
            SessionAction::Logout => "Log out of the session?",
            SessionAction::Reboot => "Reboot the machine?",
            SessionAction::Poweroff => "Shut the machine down?",
        }
    }

    /// The answer that does it, after `Cancel`.
    pub fn verb(self) -> &'static str {
        match self {
            SessionAction::Logout => "Log out",
            SessionAction::Reboot => "Reboot",
            SessionAction::Poweroff => "Shut down",
        }
    }
}

/// What choosing a row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Start a program detached from the window, which closes.
    Run {
        program: Program,
        args: &'static [Arg],
    },
    /// 32 random hex characters onto the clipboard through the secret card
    /// (spec 006), so the value never reaches a notification.
    Password,
    /// The Ask strip: a question, in this window.
    Ask,
    /// The three session-ending rows, in this window, each behind a confirm.
    Power,
    /// One of those three; constructed only by the power list's confirm.
    Session(SessionAction),
}

/// A row of the root menu (or of the palette).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// Stable, unique, and the frecency key: renaming a label keeps its history.
    pub key: &'static str,
    pub label: &'static str,
    pub detail: &'static str,
    pub glyph: Glyph,
    pub group: Group,
    pub action: Action,
}

const fn session(name: &'static str) -> Action {
    Action::Run {
        program: Program::Session(name),
        args: &[],
    }
}

const fn item(
    key: &'static str,
    label: &'static str,
    detail: &'static str,
    glyph: Glyph,
    group: Group,
    action: Action,
) -> MenuItem {
    MenuItem {
        key,
        label,
        detail,
        glyph,
        group,
        action,
    }
}

use Group::{Ai, Open, Session, System};

/// The root menu: `bar-menu`'s rows, groups and order, plus the Ask row.
pub const ROOT: &[MenuItem] = &[
    item(
        "terminal",
        "Terminal",
        "foot",
        Glyph::Terminal,
        Open,
        Action::Run {
            program: Program::System("foot"),
            args: &[],
        },
    ),
    item(
        "files",
        "Files",
        "browse the filesystem",
        Glyph::Folder,
        Open,
        session("rofi-files"),
    ),
    item(
        "files-window",
        "Files (window)",
        "favnyr: folders, drag and drop, trash",
        Glyph::Window,
        Open,
        Action::Run {
            program: Program::System("favnyr"),
            args: &[Arg::Home],
        },
    ),
    item(
        "grep",
        "Search inside files",
        "the text in them, with ripgrep",
        Glyph::Search,
        Open,
        session("rofi-grep"),
    ),
    item(
        "places",
        "Places",
        "home, disks, the configuration",
        Glyph::Disk,
        Open,
        session("rofi-places"),
    ),
    item(
        "capture",
        "Capture",
        "screenshot, OCR, QR, recording",
        Glyph::Capture,
        Open,
        session("rofi-capture"),
    ),
    item(
        "camera",
        "Camera",
        "look through the webcam, take a photo",
        Glyph::Camera,
        Open,
        session("lazyos-camera"),
    ),
    item(
        "download",
        "Download",
        "a link, into ~/Downloads",
        Glyph::Download,
        Open,
        session("rofi-download"),
    ),
    item(
        "settings",
        "Settings",
        "Wi-Fi, Bluetooth, night light, tablet mode, rotation",
        Glyph::Gear,
        Open,
        session("rofi-quick"),
    ),
    item(
        "learn",
        "Learn",
        "the manual, with search",
        Glyph::Book,
        Open,
        session("rofi-learn"),
    ),
    item(
        "keys",
        "Keybindings",
        "every binding, from the configuration",
        Glyph::Keyboard,
        Open,
        session("rofi-keys"),
    ),
    item(
        "calc",
        "Calculator",
        "type it, Enter copies the result",
        Glyph::Calculator,
        Open,
        session("rofi-calc"),
    ),
    item(
        "emoji",
        "Emoji",
        "type a name, insert the glyph",
        Glyph::Smile,
        Open,
        session("rofi-emoji"),
    ),
    item(
        "clip",
        "Clipboard",
        "what was copied, back in one pick",
        Glyph::Clipboard,
        Open,
        session("rofi-clip"),
    ),
    item(
        "password",
        "Password",
        "32 random characters, into the clipboard",
        Glyph::Key,
        Open,
        Action::Password,
    ),
    item(
        "ssh",
        "SSH",
        "hosts from ~/.ssh/config",
        Glyph::Server,
        Open,
        Action::Run {
            program: Program::System("rofi"),
            args: &[Arg::Lit("-show"), Arg::Lit("ssh")],
        },
    ),
    // The AI group opens with the two actions an agent is for: a session, and
    // one question. Both follow the default the Agents menu marks with `●`.
    item(
        "agent",
        "Agent session",
        "the default agent, in ~/Work",
        Glyph::Terminal,
        Ai,
        Action::Run {
            program: Program::Session("rofi-agents"),
            args: &[Arg::Lit("--default")],
        },
    ),
    item(
        "ask",
        "Ask",
        "one question, the answer here",
        Glyph::Lightbulb,
        Ai,
        Action::Ask,
    ),
    item(
        "agents",
        "Agents",
        "Claude Code, Codex and the others",
        Glyph::People,
        Ai,
        session("rofi-agents"),
    ),
    item(
        "dictate",
        "Dictation",
        "speak, and it is typed",
        Glyph::Mic,
        Ai,
        session("dictate"),
    ),
    // The model that runs here (spec 016): no network, no account, and the
    // flags it was measured into. Alt+Ctrl+M opens the same window.
    item(
        "model",
        "Local model",
        "a 1.7B model, offline, on four cores",
        Glyph::Chip,
        Ai,
        session("lazyos-model"),
    ),
    item(
        "wifi",
        "Wi-Fi",
        "pick a network",
        Glyph::Wifi,
        System,
        session("bar-net"),
    ),
    item(
        "qr",
        "QR code",
        "this network, or any text",
        Glyph::Qr,
        System,
        session("rofi-qr"),
    ),
    item(
        "dns",
        "DNS",
        "the resolver this connection uses",
        Glyph::Cloud,
        System,
        session("rofi-dns"),
    ),
    item(
        "bt",
        "Bluetooth",
        "pick a device",
        Glyph::Bluetooth,
        System,
        session("bar-bt"),
    ),
    item(
        "audio",
        "Audio output",
        "sinks and volume",
        Glyph::Volume,
        System,
        session("bar-audio"),
    ),
    item(
        "media",
        "Media",
        "play, pause, next",
        Glyph::Music,
        System,
        session("rofi-media"),
    ),
    item(
        "notify",
        "Notifications",
        "history and do-not-disturb",
        Glyph::Bell,
        System,
        session("rofi-notify"),
    ),
    item(
        "vnc",
        "Share this screen",
        "read-only, over ssh",
        Glyph::Eye,
        System,
        session("rofi-vnc"),
    ),
    item(
        "tz",
        "Time zone",
        "and network time",
        Glyph::Clock,
        System,
        session("rofi-tz"),
    ),
    item(
        "clean",
        "Free space",
        "what is using it, and clean up",
        Glyph::Trash,
        System,
        session("rofi-clean"),
    ),
    item(
        "swap",
        "Auxiliary swap",
        "a card as more memory, on every boot",
        Glyph::Chip,
        System,
        session("rofi-swap"),
    ),
    // The one row here that rewrites a disk. Like Power it asks again where it
    // lands: `rofi-install` confirms and then wants the disk's name typed.
    item(
        "install",
        "Install LazyOS",
        "copy this system to the internal disk - asks twice",
        Glyph::Save,
        System,
        session("rofi-install"),
    ),
    item(
        "kill",
        "End a process",
        "the heaviest twenty",
        Glyph::Bug,
        System,
        session("rofi-kill"),
    ),
    item(
        "pkg",
        "Packages",
        "search, then install one",
        Glyph::Box,
        System,
        session("rofi-pkg"),
    ),
    item(
        "dev",
        "Developer",
        "profile export and restore, diagnostics",
        Glyph::Database,
        System,
        session("rofi-dev"),
    ),
    item(
        "colour",
        "Colour",
        "the hex of a pixel, and the palette",
        Glyph::Brush,
        System,
        session("rofi-color"),
    ),
    item(
        "about",
        "About",
        "kernel, uptime, memory, battery",
        Glyph::Info,
        System,
        session("rofi-about"),
    ),
    item(
        "remind",
        "Reminders",
        "be told about it later",
        Glyph::Calendar,
        Session,
        session("rofi-remind"),
    ),
    item(
        "lock",
        "Lock",
        "the session lock",
        Glyph::Lock,
        Session,
        session("sway-lock"),
    ),
    item(
        "screensaver",
        "Screensaver",
        "the idle screen, now",
        Glyph::Display,
        Session,
        session("lazyos-screensaver"),
    ),
    item(
        "suspend",
        "Suspend",
        "sleep until asked",
        Glyph::Pause,
        Session,
        session("sway-suspend"),
    ),
    item(
        "hibernate",
        "Hibernate",
        "needs a disk swap and resume=",
        Glyph::Moon,
        Session,
        session("sway-hibernate"),
    ),
    // Log out, reboot and shut down are one row that opens their own list,
    // each behind a confirm: a second, unconfirmed copy here is how a stray
    // Enter reboots the machine (`bar-menu` said the same about rofi-power).
    item(
        "power",
        "Power",
        "log out, reboot, shut down - each asks first",
        Glyph::Power,
        Session,
        Action::Power,
    ),
];

/// The Power row's list: the three session-ending actions, in `rofi-power`'s
/// order. Choosing one opens the confirm; the confirm's second answer is the
/// only way to its [`Action::Session`].
pub const POWER: &[MenuItem] = &[
    item(
        "logout",
        "Log out",
        "back to the greeter",
        Glyph::Logout,
        Session,
        Action::Session(SessionAction::Logout),
    ),
    item(
        "reboot",
        "Reboot",
        "asks first",
        Glyph::Reboot,
        Session,
        Action::Session(SessionAction::Reboot),
    ),
    item(
        "poweroff",
        "Shut down",
        "asks first",
        Glyph::Power,
        Session,
        Action::Session(SessionAction::Poweroff),
    ),
];

/// The palette's own rows, beyond every root-menu row: the framework commands
/// that run without a question (the theme keys) and one row per built-in theme
/// (built at run time from `lazyos_theme::builtin`). The `.desktop` launcher
/// entries join in spec 012, which owns their parsing and launch path.
pub const PALETTE_EXTRA: &[MenuItem] = &[
    item(
        "theme-next",
        "Next theme",
        "the palette after this one (Alt+Shift+T)",
        Glyph::Theme,
        System,
        Action::Run {
            program: Program::Lazyos,
            args: &[Arg::Lit("theme"), Arg::Lit("next")],
        },
    ),
    item(
        "wallpaper-next",
        "Next wallpaper",
        "of the active theme (Alt+Shift+W)",
        Glyph::Image,
        System,
        Action::Run {
            program: Program::Lazyos,
            args: &[Arg::Lit("theme"), Arg::Lit("wallpaper"), Arg::Lit("next")],
        },
    ),
];

/// The `bar-menu` row keys, verbatim from the script this menu replaces (its
/// last version, before it became a symlink). The table must keep every one:
/// `the_root_menu_keeps_every_bar_menu_row` compares them, and
/// `check-system.sh` compares the same list with `lazyos menu --rows` on the
/// device.
pub const BAR_MENU_KEYS: &[&str] = &[
    "terminal",
    "files",
    "files-window",
    "grep",
    "places",
    "capture",
    "camera",
    "download",
    "settings",
    "learn",
    "keys",
    "calc",
    "emoji",
    "clip",
    "password",
    "ssh",
    "agent",
    "agents",
    "dictate",
    "model",
    "wifi",
    "qr",
    "dns",
    "bt",
    "audio",
    "media",
    "notify",
    "vnc",
    "tz",
    "clean",
    "swap",
    "install",
    "kill",
    "pkg",
    "dev",
    "colour",
    "about",
    "remind",
    "lock",
    "screensaver",
    "suspend",
    "hibernate",
    "power",
];
