//! The system pickers (spec 010): the dialogs that used to be nine scripts,
//! with one safety module behind the destructive ones.
//!
//! ```text
//! lazyos menu settings   the switches            (rofi-quick)
//! lazyos menu power      lock … shut down        (rofi-power, `menu --power`)
//! lazyos menu display    the external monitor    (rofi-display)
//! lazyos menu swap       auxiliary swap          (rofi-swap)
//! lazyos menu format     format a disk           (rofi-format)
//! lazyos menu tz         time zone               (rofi-tz)
//! lazyos menu dns        resolvers               (rofi-dns)
//! lazyos menu vnc        share this screen       (rofi-vnc)
//! lazyos menu ssh        remote login            (05-system.md)
//! ```
//!
//! Two rules hold the port together, and both are the reason the shell kept
//! getting these wrong:
//!
//! - **The rules are data and have tests.** [`disk`] decides which disks may
//!   be written (the shell's `ui_disk_system`/`ui_disks`), [`power`] decides
//!   whether this machine can hibernate and why not, [`fstab`] edits the one
//!   marked block that activates the auxiliary swap. Each takes the text the
//!   system tool printed, so a fixture from a real machine is a unit test and
//!   a changed `lsblk` field is a failing test instead of a wrong answer.
//! - **Nothing is handed to a shell.** Every call is an argv array built here,
//!   so a time zone, a device name or a resolver list read from a person is one
//!   argument and never a word a shell would split.
//!
//! The menus themselves share [`pick`]: one window, one filter line, one set
//! of keys, and a destructive row that cannot be reached without the confirm.
//!
//! The media and utility pickers (spec 011): ten dialogs that used to be
//! shell scripts, with one shared shape for the window.
//!
//! ```text
//! lazyos menu media       what is playing           (rofi-media)
//! lazyos menu notify      history and DND           (rofi-notify)
//! lazyos menu clip        clipboard by kind         (rofi-clip)
//! lazyos menu emoji       the emoji table           (rofi-emoji)
//! lazyos menu capture     screenshot, OCR, record   (rofi-capture)
//! lazyos menu qr          Wi-Fi QR or any text      (rofi-qr)
//! lazyos menu remind      write now, be told later  (rofi-remind)
//! lazyos menu calc        expression parser         (rofi-calc)
//! lazyos menu colour      pixel and palette         (rofi-color)
//! lazyos menu audio       sink list and mute        (bar-audio)
//! lazyos menu bluetooth   scan, connect, disconnect (bar-bt)
//! ```
//!
//! The rules that keep the port honest:
//! - **The rules are data and have tests.** Each picker parses the output of
//!   a system tool (`playerctl`, `cliphist`, `makoctl`, etc.) and the fixture
//!   from a real machine is the unit test. A changed tool output is a failing
//!   test instead of a wrong answer.
//! - **Nothing is handed to a shell.** Every call is an argv array built here,
//!   so a resolver list or a device name read from a person is one argument
//!   and never a word a shell would split.
//! - **One shape for the window.** The card, the note line, the rows with a
//!   glyph, and the confirm whose first answer is Cancel — the same layout
//!   the system pickers (spec 010) use, so a new picker cannot drift.
//!
//! The launcher and reference surfaces (spec 012): nine rofi modes and
//! standalone scripts, now as `lazyos menu` subcommands.
//!
//! ```text
//! lazyos menu apps        .desktop entries, icons   (rofi -show drun)
//! lazyos menu window      window switcher           (rofi -show window)
//! lazyos menu run         PATH commands, Shift+Enter→foot (rofi -show run)
//! lazyos menu keys        parsed sway config        (rofi-keys)
//! lazyos menu learn       manual + apropos          (rofi-learn)
//! lazyos menu about       fastfetch facts + logs    (rofi-about)
//! lazyos menu colour      pixel picker + palette    (rofi-color)
//! lazyos menu pkg         apt search + install      (rofi-pkg)
//! lazyos menu dev         profile export/restore    (rofi-dev)
//! ```
//!
//! The rules that keep the port honest:
//! - **The `.desktop` parser is pure and tested.** Fixtures from real
//!   `/usr/share/applications` files drive the parser; a broken entry is a
//!   failing test, not a crashed launcher.
//! - **The window switcher uses `swaymsg -t get_tree`** — recorded JSON is the
//!   fixture, so the parser works without a live compositor.
//! - **The keys parser mirrors the awk script** — same files, same comment-to-
//!   binding association, same section icons. The awk is deleted when counts match.
//! - **The markdown renderer handles the manual's subset** — headings, lists,
//!   code fences, bold, tables — with the theme's code role. No `glow` subprocess.
//! - **The revision in About equals `/etc/debian-aarch64-revision`** — the mark
//!   is sixel, cards fit at 80x24.

pub mod about;
pub mod agents;
pub mod apps;
pub mod audio;
pub mod bluetooth;
pub mod calc;
pub mod capture;
pub mod clean;
pub mod clip;
pub mod colour;
pub mod dev;
pub mod dictate;
pub mod disk;
pub mod display;
pub mod dns;
pub mod download;
pub mod emoji;
pub mod file_actions;
pub mod format;
pub mod fstab;
pub mod keys;
pub mod kill;
pub mod layout;
pub mod learn;
pub mod media;
pub mod net;
pub mod notify;
pub mod pick;
pub mod pkg;
pub mod places;
pub mod power;
pub mod qr;
pub mod remind;
pub mod run;
pub mod settings;
pub mod ssh;
pub mod step;
pub mod swap;
pub mod translate;
pub mod tz;
pub mod vnc;
pub mod window;

use std::io::Write;
use std::process::{Command, Stdio};

use lazyos_ui::{Card, Glyph, Icon, Urgency};

/// The lines a command printed, or `None` when it is missing or failed.
///
/// A failing tool is never an empty answer: every caller here treats `None` as
/// "unknown" and says so, which is what keeps a missing `nmcli` from drawing a
/// switch that reads "off".
pub fn out(argv: &[&str]) -> Option<String> {
    let (program, args) = argv.split_first()?;
    let text = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !text.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&text.stdout).trim_end().to_string())
}

/// The same, but a failure is the reason a step failed (its first stderr line,
/// or the exit code when it said nothing): a `parted` that refused has to be
/// visible, not folded into "unknown".
pub fn checked(argv: &[&str]) -> Result<String, String> {
    let (program, args) = argv.split_first().ok_or("an empty command")?;
    let text = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("could not run {program}: {error}"))?;
    if text.status.success() {
        return Ok(String::from_utf8_lossy(&text.stdout).trim_end().to_string());
    }
    let stderr = String::from_utf8_lossy(&text.stderr);
    let line = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("");
    Err(if line.is_empty() {
        format!("{program} exited {} without a message", text.status)
    } else {
        line.trim().to_string()
    })
}

/// A step that needs root. This image has passwordless sudo by design (it is
/// the recovery path), so `-n` turns a missing permission into a message
/// instead of a prompt with nowhere to appear.
pub fn sudo(argv: &[&str]) -> Result<String, String> {
    let mut line = vec!["sudo", "-n"];
    line.extend_from_slice(argv);
    checked(&line)
}

/// `command -v`, without a shell.
pub fn have(program: &str) -> bool {
    crate::files::open::on_path(&std::env::var("PATH").unwrap_or_default(), program)
}

/// The programs a picker needs before it offers anything, as one sentence the
/// window can show: a menu that opens and then fails every row is worse than
/// one that says what is missing.
pub fn missing(names: &[&str]) -> Option<String> {
    let gone: Vec<&str> = names.iter().copied().filter(|n| !have(n)).collect();
    if gone.is_empty() {
        None
    } else {
        Some(format!("Missing: {}", gone.join(" ")))
    }
}

/// Say one thing in a card, after the window is gone or when there is none.
pub fn say(title: &str, body: impl Into<String>, urgency: Urgency, glyph: Glyph) {
    let _ = Card::new(title.to_string())
        .body(body.into())
        .icon(Icon::Glyph(glyph))
        .urgency(urgency)
        .send();
}

/// The same line on stdout and in the card, so a `--status` read by
/// `check-system.sh` and the row a person reads cannot disagree.
pub fn print(line: &str) {
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
}

/// Wait for a path to exist (for the partition node after parted).
/// Used by format and swap.
pub fn wait_for(node: &str) -> bool {
    crate::system::format::wait_for(node)
}

fn busy_bin() -> std::path::PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let p = std::path::PathBuf::from(home).join(".config/bin/busy");
        if p.exists() {
            return p;
        }
    }
    std::path::PathBuf::from("busy")
}

/// Start a busy card with the bar spinner.
pub fn busy(tag: &str, title: &str, message: &str, icon: &str) {
    let _ = std::process::Command::new(busy_bin())
        .args(["start", tag, title, message, icon])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// End a busy card with success.
pub fn busy_end(tag: &str, _title: &str, message: &str) {
    let _ = std::process::Command::new(busy_bin())
        .args(["end", tag, message])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// End a busy card with failure.
pub fn busy_fail(tag: &str, _title: &str, message: &str, icon: &str) {
    let _ = std::process::Command::new(busy_bin())
        .args(["fail", tag, message, icon])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_program_is_none_not_an_empty_answer() {
        assert!(out(&["/nonexistent/lazyos-tool"]).is_none());
        assert!(checked(&["/nonexistent/lazyos-tool"]).is_err());
    }

    #[test]
    fn checked_keeps_the_reason_a_tool_gave() {
        let error =
            checked(&["sh", "-c", "printf 'second line\nlast line\n' >&2; exit 3"]).unwrap_err();
        assert_eq!(error, "last line");
    }

    #[test]
    fn a_step_that_says_nothing_still_names_itself() {
        let error = checked(&["false"]).unwrap_err();
        assert!(error.starts_with("false exited"), "{error}");
    }

    #[test]
    fn a_missing_tool_is_named_once() {
        assert_eq!(missing(&["sh", "printf"]), None);
        let said = missing(&["sh", "lazyos-not-a-program"]).unwrap();
        assert_eq!(said, "Missing: lazyos-not-a-program");
    }
}
