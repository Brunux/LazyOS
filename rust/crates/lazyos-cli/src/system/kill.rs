//! Process Manager & Killer (`rofi-kill`, `lazyos menu kill`).
//!
//! Lists current user processes with PID, command name, and memory usage.
//! Filter as you type; Enter sends SIGTERM, with follow-up SIGKILL option if
//! process refuses to terminate.

use std::process::Command;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{out, say};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRecord {
    pub pid: u32,
    pub rss_kb: u64,
    pub comm: String,
}

/// Parse `ps -eo pid=,rss=,comm= --sort=-rss` output.
pub fn parse_ps_table(raw: &str, self_pid: u32) -> Vec<ProcessRecord> {
    let mut procs = Vec::new();
    for line in raw.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let Ok(pid) = parts[0].parse::<u32>() else {
            continue;
        };
        if pid <= 1 || pid == self_pid {
            continue;
        }
        let rss_kb = parts[1].parse::<u64>().unwrap_or(0);
        let comm = parts[2..].join(" ");

        procs.push(ProcessRecord { pid, rss_kb, comm });
    }
    procs
}

/// Interactive `lazyos menu kill`.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let self_pid = std::process::id();
    let ps_out = out(&["ps", "-eo", "pid=,rss=,comm=", "--sort=-rss"]).unwrap_or_default();
    let procs = parse_ps_table(&ps_out, self_pid);

    if procs.is_empty() {
        say(
            "End a process",
            "No user processes found",
            Urgency::Normal,
            Glyph::Bug,
        );
        return Ok(0);
    }

    let mut rows = Vec::new();
    let mut keys = Vec::new();

    for p in procs.iter().take(50) {
        let mb = p.rss_kb / 1024;
        rows.push(row(&p.comm, format!("{mb} MB · pid {}", p.pid), Glyph::Bug));
        keys.push(format!("{}:{}", p.pid, p.comm));
    }

    let mut picker = Picker::new(
        "End a process",
        vec![Question::choose(
            "End a process",
            "Enter sends SIGTERM · Escape closes",
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "End a process", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    let Some(action) = answers.first() else {
        return Ok(1);
    };

    let Some((pid_str, comm)) = action.split_once(':') else {
        return Ok(1);
    };
    let Ok(pid) = pid_str.parse::<i32>() else {
        return Ok(1);
    };

    // Send SIGTERM
    let term_res = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
    if term_res.map(|s| s.success()).unwrap_or(false) {
        std::thread::sleep(std::time::Duration::from_millis(500));
        // Check if still running
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if alive {
            // Prompt for SIGKILL
            let mut kill_picker = Picker::new(
                &format!("Still running: {comm}"),
                vec![Question::choose(
                    format!("Still running: {comm}"),
                    "SIGKILL cannot be ignored, unsaved data is lost",
                    vec![
                        row(
                            "Force it (SIGKILL)",
                            "terminate immediately",
                            Glyph::Warning,
                        ),
                        row("Leave it alone", "keep process running", Glyph::Bug),
                    ],
                    vec!["kill".to_string(), "leave".to_string()],
                )],
            );
            let kc = crate::run_screen(
                terminal,
                design,
                &format!("Still running: {comm}"),
                &mut kill_picker,
            )?;
            if kc == 0 && kill_picker.answers().first().map(String::as_str) == Some("kill") {
                let _ = Command::new("kill")
                    .args(["-KILL", &pid.to_string()])
                    .status();
                say(
                    "End a process",
                    format!("{comm} ({pid}) killed"),
                    Urgency::Normal,
                    Glyph::Bug,
                );
            }
        } else {
            say(
                "End a process",
                format!("{comm} ({pid}) ended"),
                Urgency::Normal,
                Glyph::Bug,
            );
        }
        Ok(0)
    } else {
        say(
            "End a process",
            format!("Cannot signal {comm} ({pid}): it is not yours or is gone"),
            Urgency::Critical,
            Glyph::Bug,
        );
        Ok(3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ps_table_filters_pid_1_and_self() {
        let raw = "\
    1  1024 systemd
  100 20480 foot
  200 40960 rustc
  300  5120 lazyos
";
        let procs = parse_ps_table(raw, 300);
        assert_eq!(procs.len(), 2);
        assert_eq!(procs[0].pid, 100);
        assert_eq!(procs[0].comm, "foot");
        assert_eq!(procs[1].pid, 200);
        assert_eq!(procs[1].comm, "rustc");
    }
}
