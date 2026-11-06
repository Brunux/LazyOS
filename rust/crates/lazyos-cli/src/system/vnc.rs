//! Share this screen with another machine: wayvnc, read-only, on the loopback
//! address (`rofi-vnc`).
//!
//! Three decisions hold this together:
//!
//! * **Read-only.** `wayvnc -d/--disable-input` creates no virtual keyboard
//!   and no virtual pointer, so a viewer looks and cannot type.
//!
//! * **Loopback only.** wayvnc 0.10 authenticates only through
//!   ~/.config/wayvnc/config (`enable_auth` wants a certificate, a private key
//!   and a password; `enable_pam` would accept the login password). This image
//!   ships no such file, so a server on the network address would be an
//!   unauthenticated desktop on every network the machine joins. 127.0.0.1 is
//!   reached through the ssh login the image already answers, which makes the
//!   tunnel the authentication.
//!
//! * **Nothing resident.** The server is started from here and stopped from
//!   here; its pid and its log live in $XDG_RUNTIME_DIR/lazyos-vnc, so a
//!   session that dies takes the state with it and no "on" survives a logout.
//!   The bar's eye is a state file being read (waybar polls `status --bar`
//!   every ten seconds), not a process.
//!
//! The pid is checked twice before it is trusted: `kill -0` says something is
//! there, and `/proc/PID/comm` says it is wayvnc - a pid can be reused after
//! a crash, and killing a stranger would be a worse bug than a stale file.

use std::time::Duration;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{out, say};

/// What the menu needs before it offers anything.
pub const NEEDED: &[&str] = &["wayvnc", "ss"];

const PORT: u16 = 5900;
const STATE_DIR: &str = "lazyos-vnc";

/// Exact argv `start()` uses to spawn `wayvnc`.
pub const COMMAND_LINE: [&str; 5] = ["nohup", "wayvnc", "--disable-input", "127.0.0.1", "5900"];

/// The command line `start()` runs, exposed for `lazyos menu vnc --command`.
pub fn command_line() -> Vec<String> {
    COMMAND_LINE.iter().map(|s| (*s).to_string()).collect()
}

/// The SSH tunnel command a viewer runs on the other machine.
pub fn tunnel_command() -> String {
    tunnel_cmd()
}

fn state_dir() -> std::path::PathBuf {
    std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
        .join(STATE_DIR)
}

fn pid_file() -> std::path::PathBuf {
    state_dir().join("pid")
}

fn log_file() -> std::path::PathBuf {
    state_dir().join("wayvnc.log")
}

/// running: the pid file names a live wayvnc.
fn running() -> Option<u32> {
    let pid_content = std::fs::read_to_string(pid_file()).ok()?;
    let pid: u32 = pid_content.trim().parse().ok()?;
    if std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .ok()?
        .success()
        && let Ok(comm) = std::fs::read_to_string(format!("/proc/{}/comm", pid))
        && comm.trim() == "wayvnc"
    {
        return Some(pid);
    }
    None
}

/// listening: the kernel says something is bound to the loopback port.
fn listening() -> bool {
    let filter = format!("sport = :{}", PORT);
    let needle = format!("127.0.0.1:{}", PORT);
    out(&["ss", "-ltnH", &filter])
        .map(|s| s.contains(&needle))
        .unwrap_or(false)
}

fn sharing() -> bool {
    running().is_some() && listening()
}

fn tunnel_cmd() -> String {
    let host = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "lazyos".to_string());
    let user = std::env::var("USER").unwrap_or_else(|_| "brunux".to_string());
    format!("ssh -L {}:127.0.0.1:{} {}@{}", PORT, PORT, user, host)
}

fn signal_bar() {
    let _ = out(&["pkill", "-RTMIN+8", "waybar"]);
}

/// The `--status` output.
pub fn status() -> String {
    if sharing() {
        format!("localhost:{} · read-only", PORT)
    } else {
        "off".to_string()
    }
}

/// The `--status --bar` output for waybar.
pub fn status_bar() -> String {
    const EYE: &str = "\u{f06e}"; // Font Awesome eye
    if sharing() {
        format!(
            "{{\"text\":\"<span font_desc=\\'JetBrains Mono 13\\'>{}</span>\",\"class\":\"on\",\"tooltip\":\"sharing this screen read-only on localhost:{}\\nclick for the menu, right click to stop\"}}",
            EYE, PORT
        )
    } else {
        "{\"text\":\"\",\"class\":\"empty\",\"tooltip\":\"share this screen\"}".to_string()
    }
}

/// Start the wayvnc server.
pub fn start() -> Result<(), String> {
    if running().is_some() {
        return Err("Already sharing".to_string());
    }
    if !have("wayvnc") {
        return Err("wayvnc is not installed (package wayvnc)".to_string());
    }
    std::fs::create_dir_all(state_dir()).map_err(|e| e.to_string())?;

    // nohup, not setsid: `$!` has to be wayvnc's own pid, and `setsid` forks
    // when it is already a process-group leader (an interactive shell with job
    // control), which leaves the pid file naming a process that has exited
    // while the server runs on unowned. nohup execs in place and ignores
    // SIGHUP, so the pid is right in both cases and a closed terminal cannot
    // take the server with it. At logout the session's cgroup takes it.
    let mut cmd = std::process::Command::new(COMMAND_LINE[0]);
    cmd.args(&COMMAND_LINE[1..]);
    cmd.stdout(std::process::Stdio::from(
        std::fs::File::create(log_file()).map_err(|e| e.to_string())?,
    ));
    cmd.stderr(std::process::Stdio::inherit());
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    let pid = child.id();
    std::fs::write(pid_file(), pid.to_string()).map_err(|e| e.to_string())?;

    // wayvnc binds before it draws anything, so a second is generous
    std::thread::sleep(Duration::from_secs(1));
    if !sharing() {
        let reason = std::fs::read_to_string(log_file()).unwrap_or_default();
        let reason = reason.lines().rev().take(2).collect::<Vec<_>>().join(" ");
        let _ = std::process::Command::new("kill")
            .args([&pid.to_string()])
            .status();
        let _ = std::fs::remove_file(pid_file());
        return Err(format!(
            "It did not start: {}",
            if reason.is_empty() {
                "wayvnc exited without a message"
            } else {
                &reason
            }
        ));
    }
    signal_bar();
    Ok(())
}

/// Stop the wayvnc server.
pub fn stop() -> Result<(), String> {
    let pid = match running() {
        Some(p) => p,
        None => {
            let _ = std::fs::remove_file(pid_file());
            signal_bar();
            return Ok(());
        }
    };
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
    for _ in 0..30 {
        if std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| !s.success())
            .unwrap_or(true)
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
    let _ = std::fs::remove_file(pid_file());
    signal_bar();
    Ok(())
}

/// `lazyos menu vnc` (`rofi-vnc`): the VNC picker.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Share this screen",
            format!("{said} (wayvnc, iproute2)"),
            Urgency::Critical,
            Glyph::Eye,
        );
        return Ok(3);
    }

    if sharing() {
        // Already sharing: offer stop and tunnel
        let mut picker = Picker::new(
            "Share this screen",
            vec![Question::choose(
                "sharing",
                "Enter acts · Escape closes",
                vec![
                    row(
                        "Sharing this screen",
                        format!("localhost:{} · read-only", PORT),
                        Glyph::Eye,
                    ),
                    row("Stop sharing", "the viewer loses the picture", Glyph::Stop),
                    row("Copy the tunnel command", tunnel_cmd(), Glyph::Link),
                    row(
                        "What this does",
                        "a viewer looks; it cannot type, and only over ssh",
                        Glyph::Info,
                    ),
                ],
                vec!["none".into(), "stop".into(), "tunnel".into(), "info".into()],
            )],
        );
        let code = crate::run_screen(terminal, design, "Share this screen", &mut picker)?;
        if code != 0 {
            return Ok(code);
        }
        let answers = picker.answers();
        if answers.is_empty() {
            return Ok(1);
        }
        match answers[0].as_str() {
            "stop" => {
                if let Err(e) = stop() {
                    say(
                        "Share this screen",
                        format!("Could not stop: {}", e),
                        Urgency::Critical,
                        Glyph::Eye,
                    );
                    return Ok(3);
                }
                say(
                    "Share this screen",
                    "Stopped. The viewer loses the picture.".to_string(),
                    Urgency::Normal,
                    Glyph::Eye,
                );
                Ok(0)
            }
            "tunnel" => {
                // Copy to clipboard
                let cmd = tunnel_cmd();
                let _ = out(&["wl-copy", &cmd]);
                say(
                    "Share this screen",
                    format!(
                        "Run on the other machine, then open a VNC viewer on localhost:{}",
                        PORT
                    ),
                    Urgency::Normal,
                    Glyph::Eye,
                );
                Ok(0)
            }
            "info" => {
                say(
                    "Share this screen",
                    format!(
                        "wayvnc serves this session on 127.0.0.1:{}, read-only: the viewer sees the screen and cannot type. It is loopback only because wayvnc 0.10 needs a certificate and a password file to serve a network, which this image does not ship - the ssh login is what authenticates, and the tunnel row has the command. Nothing runs until it is started, and the bar keeps an eye while it is.",
                        PORT
                    ),
                    Urgency::Normal,
                    Glyph::Eye,
                );
                Ok(0)
            }
            _ => Ok(0),
        }
    } else {
        // Not sharing: offer start
        let mut picker = Picker::new(
            "Share this screen",
            vec![Question::choose(
                "Share this screen",
                "read-only · localhost:5900 · watched from another machine",
                vec![
                    row(
                        "Share this screen",
                        "read-only · localhost:5900 · watched from another machine",
                        Glyph::Eye,
                    ),
                    row("Copy the tunnel command", tunnel_cmd(), Glyph::Link),
                    row(
                        "What this does",
                        "a viewer looks; it cannot type, and only over ssh",
                        Glyph::Info,
                    ),
                ],
                vec!["start".into(), "tunnel".into(), "info".into()],
            )],
        );
        let code = crate::run_screen(terminal, design, "Share this screen", &mut picker)?;
        if code != 0 {
            return Ok(code);
        }
        let answers = picker.answers();
        if answers.is_empty() {
            return Ok(1);
        }
        match answers[0].as_str() {
            "start" => {
                if let Err(e) = start() {
                    say(
                        "Share this screen",
                        format!("It did not start: {}", e),
                        Urgency::Critical,
                        Glyph::Eye,
                    );
                    return Ok(3);
                }
                say(
                    "Share this screen",
                    format!(
                        "Read-only on localhost:{}. From the other machine: {}",
                        PORT,
                        tunnel_cmd()
                    ),
                    Urgency::Normal,
                    Glyph::Eye,
                );
                Ok(0)
            }
            "tunnel" => {
                let cmd = tunnel_cmd();
                let _ = out(&["wl-copy", &cmd]);
                say(
                    "Share this screen",
                    format!(
                        "Run on the other machine, then open a VNC viewer on localhost:{}",
                        PORT
                    ),
                    Urgency::Normal,
                    Glyph::Eye,
                );
                Ok(0)
            }
            "info" => {
                say(
                    "Share this screen",
                    format!(
                        "wayvnc serves this session on 127.0.0.1:{}, read-only: the viewer sees the screen and cannot type. It is loopback only because wayvnc 0.10 needs a certificate and a password file to serve a network, which this image does not ship - the ssh login is what authenticates, and the tunnel row has the command. Nothing runs until it is started, and the bar keeps an eye while it is.",
                        PORT
                    ),
                    Urgency::Normal,
                    Glyph::Eye,
                );
                Ok(0)
            }
            _ => Ok(0),
        }
    }
}

fn missing(names: &[&str]) -> Option<String> {
    let gone: Vec<&str> = names.iter().copied().filter(|n| !have(n)).collect();
    if gone.is_empty() {
        None
    } else {
        Some(format!("Missing: {}", gone.join(" ")))
    }
}

fn have(program: &str) -> bool {
    crate::files::open::on_path(&std::env::var("PATH").unwrap_or_default(), program)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunnel_cmd_contains_port() {
        let cmd = tunnel_cmd();
        assert!(cmd.contains(&PORT.to_string()));
        assert!(cmd.contains("ssh -L"));
    }

    #[test]
    fn command_line_is_read_only_loopback() {
        let cmd = command_line().join(" ");
        assert!(cmd.contains("--disable-input"));
        assert!(cmd.contains("127.0.0.1"));
        assert!(!cmd.contains("0.0.0.0"));
    }
}
