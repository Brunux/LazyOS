//! Keyboard Layout Switcher (`rofi-layout`, `lazyos menu layout`).
//!
//! Lists supported keyboard layouts from xkb's `evdev.lst`, marks the active
//! layout, and applies choice via Sway IPC (`swaymsg input type:keyboard xkb_layout <layout>`),
//! persisting it to `~/.config/sway/config.d/90-keyboard-layout.conf`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{out, say};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutEntry {
    pub code: String,
    pub name: String,
}

/// Parse `/usr/share/X11/xkb/rules/evdev.lst` section `! layout`.
pub fn parse_evdev_lst(raw: &str) -> Vec<LayoutEntry> {
    let mut layouts = Vec::new();
    let mut in_section = false;

    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with("! layout") {
            in_section = true;
            continue;
        } else if line.starts_with('!') {
            in_section = false;
        }

        if in_section && !line.is_empty() {
            let mut parts = line.split_whitespace();
            if let Some(code) = parts.next() {
                let name = parts.collect::<Vec<&str>>().join(" ");
                if !name.is_empty() {
                    layouts.push(LayoutEntry {
                        code: code.to_string(),
                        name,
                    });
                }
            }
        }
    }
    layouts
}

/// Get the active keyboard layout name from Sway.
pub fn active_layout() -> Option<String> {
    let raw = out(&["swaymsg", "-t", "get_inputs", "-r"])?;
    // Quick search for xkb_active_layout_name in JSON
    let key = "\"xkb_active_layout_name\":";
    if let Some(pos) = raw.find(key) {
        let after = &raw[pos + key.len()..];
        let start = after.find('"')? + 1;
        let end = after[start..].find('"')?;
        return Some(after[start..start + end].to_string());
    }
    None
}

/// Set active keyboard layout and persist to sway config drop-in.
pub fn set_layout(code: &str) -> Result<(), String> {
    let res = Command::new("swaymsg")
        .args(["input", "type:keyboard", "xkb_layout", code])
        .output()
        .map_err(|e| e.to_string())?;

    if !res.status.success() {
        return Err(String::from_utf8_lossy(&res.stderr).trim().to_string());
    }

    let base_config = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".config")
        });
    let config_dir = base_config.join("sway/config.d");
    let _ = fs::create_dir_all(&config_dir);
    let drop_in = config_dir.join("90-keyboard-layout.conf");
    let content = format!(
        "# Written by rofi-layout: the keyboard layout picked in the session.\n\
         # Delete this file to go back to whatever the system configuration sets.\n\
         input \"type:keyboard\" xkb_layout \"{code}\"\n"
    );
    let _ = fs::write(drop_in, content);
    Ok(())
}

/// Interactive `lazyos menu layout`.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let rules_path = "/usr/share/X11/xkb/rules/evdev.lst";
    let Ok(content) = fs::read_to_string(rules_path) else {
        say(
            "Keyboard layout",
            "evdev.lst is missing (package xkb-data)",
            Urgency::Critical,
            Glyph::Keyboard,
        );
        return Ok(3);
    };

    let layouts = parse_evdev_lst(&content);
    let active = active_layout().unwrap_or_default();

    let mut rows = Vec::new();
    let mut keys = Vec::new();

    for l in &layouts {
        let is_active = l.name == active || l.code == active;
        let mark = if is_active { "● " } else { "  " };
        let detail = if is_active {
            format!("{} · current", l.code)
        } else {
            l.code.clone()
        };
        rows.push(row(format!("{}{}", mark, l.name), detail, Glyph::Keyboard));
        keys.push(l.code.clone());
    }

    let mut picker = Picker::new(
        "Keyboard layout",
        vec![Question::choose(
            "Keyboard layout",
            if active.is_empty() {
                "Enter sets layout · Escape closes".to_string()
            } else {
                format!("Current: {active} · Enter sets layout · Escape closes")
            },
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Keyboard layout", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    let Some(chosen) = answers.first() else {
        return Ok(1);
    };

    match set_layout(chosen) {
        Ok(()) => {
            say(
                "Keyboard layout",
                format!("Now using {chosen}"),
                Urgency::Normal,
                Glyph::Keyboard,
            );
            Ok(0)
        }
        Err(err) => {
            say(
                "Keyboard layout",
                format!("Could not set {chosen}: {err}"),
                Urgency::Critical,
                Glyph::Keyboard,
            );
            Ok(3)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_evdev_lst_extracts_codes_and_names() {
        let raw = "\
! model
  pc105           Generic 105-key PC

! layout
  us              English (US)
  es              Spanish
  latam           Spanish (Latin American)

! variant
  dvorak          us: English (Dvorak)
";
        let list = parse_evdev_lst(raw);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].code, "us");
        assert_eq!(list[0].name, "English (US)");
        assert_eq!(list[1].code, "es");
        assert_eq!(list[1].name, "Spanish");
        assert_eq!(list[2].code, "latam");
        assert_eq!(list[2].name, "Spanish (Latin American)");
    }
}
