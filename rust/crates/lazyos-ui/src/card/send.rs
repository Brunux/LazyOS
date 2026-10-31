//! How a card reaches mako: `notify-send`, with an argument vector.
//!
//! Every card the shell sends goes through `notify-send`, and so does this one:
//! the same program, the same `Notify` call on the session bus, the same hints.
//! What the binary adds is what a shell cannot do cleanly - it reads the id mako
//! assigned (`-p`), it reports a refusal with a reason, and the text is passed
//! as an argument vector after `--`, so a title that starts with a dash is text
//! and never an option.
//!
//! The choice is measured (`docs/optimization.md` §14). A bus client in the
//! binary was tried first: it added 42 crates, 2.6 MB to the release binary and
//! about 1m40s to a clean build, for a call the shell already makes. The cost
//! here is one short process per card, and nothing in the build.

use std::path::Path;
use std::process::Command;

use super::{APP_NAME, GROUP_HINT, Message};

const PROGRAM: &str = "notify-send";

/// Post the message through `notify-send` on the session bus and return the id
/// mako assigned.
pub fn notify(message: &Message) -> Result<u32, String> {
    notify_with(Path::new(PROGRAM), message)
}

/// [`notify`] with the program named, so a test can put a stub in its place.
pub fn notify_with(program: &Path, message: &Message) -> Result<u32, String> {
    let output = Command::new(program)
        .args(arguments(message))
        .output()
        .map_err(|error| format!("cannot run {}: {error}", program.display()))?;
    if !output.status.success() {
        return Err(describe(&String::from_utf8_lossy(&output.stderr)));
    }
    let printed = String::from_utf8_lossy(&output.stdout);
    printed
        .trim()
        .parse()
        .map_err(|_| format!("notify-send printed no id: {:?}", printed.trim()))
}

/// The argument vector: what `notify-send` is asked to send.
pub fn arguments(message: &Message) -> Vec<String> {
    let mut args = vec![
        "-a".to_string(),
        APP_NAME.to_string(),
        "-u".to_string(),
        message.urgency.name().to_string(),
        format!("--expire-time={}", message.expire_ms),
        "-p".to_string(),
    ];
    if !message.app_icon.is_empty() {
        args.push("-i".to_string());
        args.push(message.app_icon.clone());
    }
    if let Some(tag) = &message.group {
        args.push("-h".to_string());
        args.push(format!("string:{GROUP_HINT}:{tag}"));
    }
    if let Some(value) = message.value {
        args.push("-h".to_string());
        args.push(format!("int:value:{value}"));
    }
    args.push("--".to_string());
    args.push(message.summary.clone());
    args.push(message.body.clone());
    args
}

/// The reason a refused call gives, in words a person can act on. The bus
/// errors are matched in `notify-send`'s own text.
fn describe(stderr: &str) -> String {
    if stderr.contains("ServiceUnknown") || stderr.contains("NameHasNoOwner") {
        "no notification daemon answers on the session bus (is mako running?)".to_string()
    } else if stderr.contains("Could not connect") {
        "no session bus (DBUS_SESSION_BUS_ADDRESS does not reach one)".to_string()
    } else {
        let line = stderr.lines().next().unwrap_or_default().trim();
        if line.is_empty() {
            "notify-send failed without a reason".to_string()
        } else {
            format!("notify-send failed: {line}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::{Card, Urgency};
    use std::os::unix::fs::PermissionsExt;

    fn message() -> Message {
        Card::new("--Volume")
            .body("40%")
            .urgency(Urgency::Low)
            .progress(40)
            .group("osd")
            .prepare(&[])
            .message
    }

    /// A stand-in `notify-send`: it records its arguments one per line in
    /// `log`, then does what `reply` says (prints an id, or refuses).
    fn stub(label: &str, reply: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("lazyos-send-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("args.txt");
        let program = dir.join("notify-send");
        let scratch = dir.join("notify-send.new");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n{reply}\n",
            log.display()
        );
        // Written through a scratch name and renamed: another test thread can
        // fork while this file is open for writing, the child inherits that
        // descriptor, and the next `exec` fails with `ETXTBSY`.
        std::fs::write(&scratch, script).unwrap();
        let mut perms = std::fs::metadata(&scratch).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&scratch, perms).unwrap();
        std::fs::rename(&scratch, &program).unwrap();
        (program, log)
    }

    #[test]
    fn the_arguments_carry_the_hints_the_shell_sends() {
        let args = arguments(&message());
        let has = |pair: &[&str]| args.windows(pair.len()).any(|w| w == pair);
        assert!(has(&["-a", "LazyOS"]));
        assert!(has(&["-u", "low"]));
        assert!(has(&["--expire-time=-1"]));
        assert!(has(&["-p"]), "mako's id comes back on stdout");
        assert!(has(&[
            "-h",
            "string:x-canonical-private-synchronous:lazyos-osd"
        ]));
        assert!(has(&["-h", "int:value:40"]));
        assert!(!has(&["-i"]), "no picture was asked for");
    }

    #[test]
    fn a_title_that_starts_with_a_dash_is_text_after_the_terminator() {
        let args = arguments(&message());
        let end = args.iter().position(|a| a == "--").expect("the terminator");
        assert_eq!(args[end + 1], "--Volume");
        assert_eq!(args[end + 2], "40%");
        assert!(args[..end].iter().all(|a| a != "--Volume"));
    }

    #[test]
    fn a_picture_is_passed_with_dash_i() {
        let theme = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../uis/sway/files/usr/share/icons/LazyOS");
        let with = Card::new("Saved")
            .icon(crate::card::Icon::Name("folder".into()))
            .prepare(&[theme])
            .message;
        let args = arguments(&with);
        let at = args.iter().position(|a| a == "-i").expect("-i is passed");
        assert_eq!(args[at + 1], "folder");
        let without = Card::new("Saved").no_icon().prepare(&[]).message;
        assert!(!arguments(&without).iter().any(|a| a == "-i"));
    }

    #[test]
    fn the_reply_is_the_id_notify_send_printed_and_the_call_is_recorded() {
        let (program, log) = stub("reply", "echo 7");
        let id = notify_with(&program, &message()).expect("the stub answers");
        assert_eq!(id, 7);
        let recorded = std::fs::read_to_string(&log).unwrap();
        assert!(
            recorded.lines().any(|line| line == "--Volume"),
            "{recorded}"
        );
        assert!(
            recorded.lines().any(|line| line == "int:value:40"),
            "{recorded}"
        );
        let _ = std::fs::remove_dir_all(program.parent().unwrap());
    }

    #[test]
    fn a_refusal_names_the_missing_daemon() {
        let (program, _) = stub(
            "refused",
            "echo 'Failed to show notification: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown: nobody' >&2\necho 0\nexit 1",
        );
        let reason = notify_with(&program, &message()).unwrap_err();
        assert!(reason.contains("no notification daemon"), "{reason}");
        let _ = std::fs::remove_dir_all(program.parent().unwrap());
    }

    #[test]
    fn a_missing_program_is_reported_not_ignored() {
        let reason = notify_with(Path::new("/nonexistent/notify-send"), &message()).unwrap_err();
        assert!(
            reason.starts_with("cannot run /nonexistent/notify-send"),
            "{reason}"
        );
    }
}
