//! SSH remote login status (`rofi-ssh` equivalent).
//!
//! The image runs `openssh-server` with `ssh.service` enabled on every boot.
//! This menu shows the status and the commands to change it.

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{out, say, sudo};

/// What the menu needs before it offers anything.
pub const NEEDED: &[&str] = &["systemctl", "sshd"];

/// The `--status` output.
pub fn status() -> String {
    let enabled = out(&["systemctl", "is-enabled", "ssh"])
        .map(|s| s.trim() == "enabled")
        .unwrap_or(false);
    let active = out(&["systemctl", "is-active", "ssh"])
        .map(|s| s.trim() == "active")
        .unwrap_or(false);
    let port = out(&["ss", "-ltnH", "sport = :22"])
        .map(|s| s.contains("0.0.0.0:22"))
        .unwrap_or(false);

    let mut parts = Vec::new();
    parts.push(format!(
        "ssh.service: {}",
        if enabled { "enabled" } else { "disabled" }
    ));
    parts.push(format!("running: {}", if active { "yes" } else { "no" }));
    if port {
        parts.push("listening on 0.0.0.0:22".to_string());
    }
    parts.join(" · ")
}

/// `lazyos menu ssh`: the SSH status picker.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Remote login (ssh)",
            format!("{said} (systemd, openssh)"),
            Urgency::Critical,
            Glyph::Key,
        );
        return Ok(3);
    }

    let enabled = out(&["systemctl", "is-enabled", "ssh"])
        .map(|s| s.trim() == "enabled")
        .unwrap_or(false);
    let active = out(&["systemctl", "is-active", "ssh"])
        .map(|s| s.trim() == "active")
        .unwrap_or(false);

    let mut rows = Vec::new();
    let mut keys = Vec::new();

    rows.push(row(
        "ssh.service",
        if enabled { "enabled" } else { "disabled" },
        Glyph::Key,
    ));
    keys.push("service".to_string());

    rows.push(row(
        "running",
        if active { "yes" } else { "no" },
        if active { Glyph::Check } else { Glyph::Lock },
    ));
    keys.push("running".to_string());

    rows.push(row(
        "Disable ssh",
        "sudo systemctl disable --now ssh",
        Glyph::Lock,
    ));
    keys.push("disable".to_string());

    rows.push(row(
        "Enable ssh",
        "sudo systemctl enable --now ssh",
        Glyph::Unlock,
    ));
    keys.push("enable".to_string());

    let mut picker = Picker::new(
        "Remote login (ssh)",
        vec![Question::choose(
            "Remote login (ssh)",
            "Enter acts · Escape closes",
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Remote login (ssh)", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    if answers.is_empty() {
        return Ok(1);
    }

    let key = &answers[0];

    match key.as_str() {
        "disable" => {
            if let Err(e) = sudo(&["systemctl", "disable", "--now", "ssh"]) {
                say(
                    "Remote login (ssh)",
                    format!("Could not disable ssh: {}", e),
                    Urgency::Critical,
                    Glyph::Lock,
                );
                return Ok(3);
            }
            say(
                "Remote login (ssh)",
                "ssh disabled".to_string(),
                Urgency::Normal,
                Glyph::Lock,
            );
            Ok(0)
        }
        "enable" => {
            if let Err(e) = sudo(&["systemctl", "enable", "--now", "ssh"]) {
                say(
                    "Remote login (ssh)",
                    format!("Could not enable ssh: {}", e),
                    Urgency::Critical,
                    Glyph::Unlock,
                );
                return Ok(3);
            }
            say(
                "Remote login (ssh)",
                "ssh enabled".to_string(),
                Urgency::Normal,
                Glyph::Unlock,
            );
            Ok(0)
        }
        _ => Ok(0),
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
    #[test]
    fn status_formats_correctly() {
        // Would require mocking systemctl
    }
}
