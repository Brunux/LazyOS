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

pub mod disk;
pub mod display;
pub mod dns;
pub mod format;
pub mod fstab;
pub mod pick;
pub mod power;
pub mod step;
pub mod swap;
pub mod tz;

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
        let error = checked(&[
            "sh",
            "-c",
            "printf 'second line\\nlast line\\n' >&2; exit 3",
        ])
        .unwrap_err();
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
