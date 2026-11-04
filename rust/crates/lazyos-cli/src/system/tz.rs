//! Time zone: every zone tzdata knows, with the current one marked (`rofi-tz`).
//!
//! The list is `timedatectl list-timezones` — 400-odd rows that rofi filters
//! as fast as anything else — and the network-time switch is the row at the
//! end, because a clock that is wrong and a clock that does not sync are the
//! same problem (systemd-timesyncd is in packages/base.list for that).
//!
//! sudo is passwordless on this image, hence -n: if it ever is not, the failure
//! is a notification instead of a password prompt with nowhere to appear.

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{out, say, sudo};

/// What the menu needs before it offers anything.
pub const NEEDED: &[&str] = &["timedatectl"];

/// The `--status` output.
pub fn status() -> String {
    let current = out(&["timedatectl", "show", "-p", "Timezone", "--value"]).unwrap_or_default();
    let ntp = out(&["timedatectl", "show", "-p", "NTP", "--value"]).unwrap_or_default();
    let ntp_str = if ntp == "yes" { "on" } else { "off" };
    format!("Current: {} · Network time: {}", current, ntp_str)
}

/// `lazyos menu tz` (`rofi-tz`): the time zone picker.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say("Time zone", format!("{said} (systemd)"), Urgency::Critical, Glyph::Clock);
        return Ok(3);
    }

    let current = out(&["timedatectl", "show", "-p", "Timezone", "--value"]).unwrap_or_default();
    let ntp = out(&["timedatectl", "show", "-p", "NTP", "--value"]).unwrap_or_default();

    let zones = out(&["timedatectl", "list-timezones"]).unwrap_or_default();
    let mut rows = Vec::new();
    let mut keys = Vec::new();

    for zone in zones.lines() {
        let zone = zone.trim();
        if zone.is_empty() {
            continue;
        }
        if zone == current {
            rows.push(row(zone, "current", Glyph::Clock));
        } else {
            rows.push(row(zone, "", Glyph::Clock));
        }
        keys.push(format!("set:{}", zone));
    }

    if ntp == "yes" {
        rows.push(row("Set the clock from the network", "on", Glyph::Clock));
        keys.push("ntp:off".into());
    } else {
        rows.push(row("Set the clock from the network", "off", Glyph::Clock));
        keys.push("ntp:on".into());
    }

    let mut picker = Picker::new(
        "Time zone",
        vec![Question::choose(
            "Time zone",
            format!("Current: {} · Enter sets · Escape closes", current),
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Time zone", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    if answers.is_empty() {
        return Ok(1);
    }
    let key = &answers[0];

    match key.as_str() {
        k if k.starts_with("set:") => {
            let zone = &k[4..];
            if let Err(e) = sudo(&["timedatectl", "set-timezone", zone]) {
                say("Time zone", format!("Could not set {}: {}", zone, e), Urgency::Critical, Glyph::Clock);
                return Ok(3);
            }
            // timedatectl rewrites /etc/localtime, and that is all it does for a
            // running session: glibc caches the zone the first time localtime() is
            // called in a process, so the bar's clock kept showing the old time and
            // the menu looked like it had done nothing. Restart the bar (it is the
            // one long-lived process in the session that displays the time), and put
            // the new local time in the notification so the change is visible even
            // before the bar is back.
            if out(&["pgrep", "-x", "waybar"]).is_some() {
                let _ = out(&["pkill", "-x", "waybar"]);
                let _ = std::process::Command::new("setsid").arg("waybar").spawn();
            }
            let now = out(&["sh", "-c", &format!("TZ={} date '+%H:%M'", zone)]).unwrap_or_default();
            say("Time zone", format!("Now {} · {}", zone, now), Urgency::Normal, Glyph::Clock);
            Ok(0)
        }
        "ntp:on" | "ntp:off" => {
            let want = if key == "ntp:on" { "true" } else { "false" };
            if let Err(e) = sudo(&["timedatectl", "set-ntp", want]) {
                say("Time zone", format!("Could not change network time: {}", e), Urgency::Critical, Glyph::Clock);
                return Ok(3);
            }
            say("Time zone", format!("Network time {}", want), Urgency::Normal, Glyph::Clock);
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
        // Would require mocking timedatectl
    }
}