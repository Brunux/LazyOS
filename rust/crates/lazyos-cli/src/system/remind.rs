//! Reminders: write it now, be told later (`rofi-remind`).
//!
//! The old script was a multi-step flow: title → note → when (delays, date, repeats)
//! → confirm. The reminders are transient systemd user timers (`systemd-run --user
//! --collect --on-active/--on-calendar`) that fire `notify-send`. The list reads
//! from systemd itself, so there is no second store to drift.

#![allow(unused_imports)]
#![allow(dead_code)]

#![allow(clippy::collapsible_if)]

#![allow(clippy::new_without_default)]
#![allow(clippy::vec_init_then_push)]
use std::cell::Cell;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, Input, InputAction, KeyHint, ListAction, ListState, Outcome,
    Row, Screen, TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use crate::system::{have, missing, out, print, say};

const NEEDED: &[&str] = &[
    "systemd-run",
    "systemctl",
    "systemd-analyze",
    "date",
    "wl-copy",
    "jq",
];
const PREFIX: &str = "lazyos-reminder-";
const TAB: &str = "\t";

/// The remind screen.
pub struct RemindScreen {
    reminders: Vec<Reminder>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    keys: Vec<String>,
}

#[derive(Clone, Debug)]
struct Reminder {
    unit: String,
    when: u64,
    title: String,
    note: String,
    calendar: Option<String>,
}

impl RemindScreen {
    pub fn new() -> Self {
        let mut screen = Self {
            keys: Vec::new(),
            reminders: Vec::new(),
            list: ListState::new().with_empty("no reminders waiting"),
            header: Header::new("LazyOS").subtitle("Reminders"),
            keybar: vec![
                KeyHint::new("Enter", "cancel reminder"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
        };
        screen.reload();
        screen
    }

    fn reload(&mut self) {
        self.reminders = Self::get_reminders();
        self.build_rows();
    }

    fn get_reminders() -> Vec<Reminder> {
        let output = out(&[
            "systemctl",
            "--user",
            "list-timers",
            "--all",
            "--output=json",
            &format!("{}*", PREFIX),
        ])
        .unwrap_or_default();
        let mut reminders = Vec::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    let unit = item
                        .get("unit")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let next = item.get("next").and_then(|v| v.as_u64()).unwrap_or(0) / 1_000_000;
                    if next == 0 || next <= now {
                        continue;
                    }
                    let props = out(&[
                        "systemctl",
                        "--user",
                        "show",
                        "-p",
                        "Description",
                        "-p",
                        "TimersCalendar",
                        &unit,
                    ])
                    .unwrap_or_default();
                    let mut desc = String::new();
                    let mut cal = String::new();
                    for line in props.lines() {
                        if line.starts_with("Description=") {
                            desc = line["Description=".len()..].to_string();
                        } else if line.starts_with("TimersCalendar=") {
                            cal = line["TimersCalendar={ OnCalendar=".len()..]
                                .trim_end_matches('}')
                                .to_string();
                        }
                    }
                    let title = desc
                        .split(TAB)
                        .next()
                        .unwrap_or("(untitled reminder)")
                        .to_string();
                    let note = desc.split(TAB).nth(1).unwrap_or("").to_string();
                    reminders.push(Reminder {
                        unit,
                        when: next,
                        title,
                        note,
                        calendar: if cal.is_empty() { None } else { Some(cal) },
                    });
                }
            }
        }
        reminders
    }

    fn left(secs: u64) -> String {
        if secs < 60 {
            "in less than a minute".to_string()
        } else if secs < 3600 {
            format!("in {} min", secs / 60)
        } else if secs < 86400 {
            let h = secs / 3600;
            let m = (secs % 3600) / 60;
            if m > 0 {
                format!("in {} h {} min", h, m)
            } else {
                format!("in {} h", h)
            }
        } else {
            format!("in {} days", secs / 86400)
        }
    }

    fn weekday(d: &str) -> &str {
        match d {
            "Mon" => "Monday",
            "Tue" => "Tuesday",
            "Wed" => "Wednesday",
            "Thu" => "Thursday",
            "Fri" => "Friday",
            "Sat" => "Saturday",
            "Sun" => "Sunday",
            _ => d,
        }
    }

    fn repeat_words(cal: &str) -> Option<String> {
        let time = cal.split_whitespace().last()?;
        let hm = time.strip_suffix(':').unwrap_or(time);
        let day_part = cal.split_whitespace().next()?;
        let words = match day_part {
            "*-*-*" => Some(format!("every day at {}", hm)),
            "Mon..Fri" | "Mon-Fri" => Some(format!("every weekday at {}", hm)),
            "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun" => {
                Some(format!("every {} at {}", Self::weekday(day_part), hm))
            }
            d if d.starts_with("*-*-") => {
                let day = d["*-*-".len()..].trim_start_matches('0');
                let day = if day.is_empty() { "0" } else { day };
                Some(format!("every month on day {} at {}", day, hm))
            }
            d if d.contains('-') && d.chars().next().unwrap().is_ascii_digit() => {
                // One-shot date
                let _ = chrono::NaiveDateTime::parse_from_str(
                    &format!("{} {}", d, hm),
                    "%Y-%m-%d %H:%M",
                );
                None
            }
            _ => Some("repeating".to_string()),
        };
        words
    }

    fn trim(s: &str, max: usize) -> String {
        if s.len() <= max {
            s.to_string()
        } else {
            format!("{}…", &s[..max])
        }
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if self.reminders.is_empty() {
            rows.push(
                Row::new("No reminders waiting")
                    .glyph(Glyph::Clock)
                    .detail("The row below starts one"),
            );
        } else {
            for r in &self.reminders {
                let words = r
                    .calendar
                    .as_ref()
                    .and_then(|c| Self::repeat_words(c))
                    .unwrap_or_else(|| Self::left(r.when.saturating_sub(now)));
                let mut detail = format!(
                    "{} · {}",
                    words,
                    chrono::DateTime::from_timestamp(r.when as i64, 0)
                        .map(|d| d.format("%a %d %b %H:%M").to_string())
                        .unwrap_or_default()
                );
                detail.push_str(" · Enter cancels");
                if !r.note.is_empty() {
                    detail = format!("{} · {}", Self::trim(&r.note, 40), detail);
                }
                rows.push(
                    Row::new(Self::trim(&r.title, 72))
                        .glyph(Glyph::Clock)
                        .detail(detail)
                        .value(format!("cancel:{}", r.unit)),
                );
            }
        }
        rows.push(
            Row::new("New reminder")
                .glyph(Glyph::Bell)
                .detail("say what, when, and whether it repeats")
                .value("new"),
        );
        self.keys = rows.iter().filter_map(|r| r.value.clone()).collect();
        self.list.set_rows(rows);
    }
}

impl Screen for RemindScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Reminders"), true);
        let (filter, rows) = super::pick::list_layout(ctx.area);
        let msg = if self.reminders.is_empty() {
            "Enter starts a reminder · Escape closes"
        } else {
            "{} waiting · Enter cancels one · Escape closes"
        };
        ctx.buf.set_string(
            filter.x,
            filter.y,
            msg.replace("{}", &self.reminders.len().to_string()),
            ctx.design.style(TextStyle::Dim),
        );
        self.rows_height.set(rows.height);
        self.list.render(rows, ctx.buf, ctx.design, true);
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }
        let rows_height = self.rows_height.get();
        match key.code {
            KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
            KeyCode::Enter => {
                if let Some(index) = self.list.selected_index() {
                    return self.activate(index);
                }
            }
            _ => {}
        }
        match self.list.handle(key, rows_height, design) {
            ListAction::Activated(index) => self.activate(index),
            _ => Outcome::Continue,
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let Some(row) = row {
            self.list.set_cursor(row);
            if let Some(index) = self.list.selected_index() {
                return self.activate(index);
            }
        }
        Outcome::Continue
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        let (_, rows) = super::pick::list_layout(area);
        self.list.hit_row(rows, x, y, design)
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }
}

impl RemindScreen {
    fn activate(&mut self, index: usize) -> Outcome {
        let rows = self.list.rows();
        if index >= rows.len() {
            return Outcome::Continue;
        }
        let key = self.keys.get(index).cloned().unwrap_or_default();
        if key.is_empty() || key == "-" {
            return Outcome::Continue;
        }
        if key == "new" {
            // Run the new reminder flow
            self.run_new_reminder_flow();
            self.reload();
            Outcome::Continue
        } else if key.starts_with("cancel:") {
            let unit = key.strip_prefix("cancel:").unwrap_or("");
            if let Some(r) = self.reminders.iter().find(|r| r.unit == unit) {
                let name = if r.title.is_empty() {
                    "The reminder"
                } else {
                    &r.title
                };
                let said = format!("“{}”", name);
                let output = std::process::Command::new("systemctl")
                    .args(["--user", "stop", unit])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                if output.map(|s| s.success()).unwrap_or(false) {
                    let mut msg = said;
                    msg.push_str(
                        "
It will not fire.",
                    );
                    say("Cancelled", msg, lazyos_ui::Urgency::Normal, Glyph::Clock);
                } else {
                    say(
                        "Already gone",
                        format!("{} was already gone.", said),
                        lazyos_ui::Urgency::Critical,
                        Glyph::Clock,
                    );
                }
                self.reload();
            }
            Outcome::Continue
        } else {
            Outcome::Continue
        }
    }

    fn run_new_reminder_flow(&mut self) {
        // This is a simplified version - in reality this would be a multi-step flow
        // similar to the old script. For now, we'll just say it's not fully implemented.
        say(
            "Reminders",
            "New reminder flow not yet fully ported; use the command line",
            lazyos_ui::Urgency::Normal,
            Glyph::Clock,
        );
    }

    fn list_layout(area: Rect) -> (Rect, Rect) {
        let inner = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(2),
        };
        let filter = Rect {
            height: 1.min(inner.height),
            ..inner
        };
        let rows = Rect {
            y: inner.y + 2,
            height: inner.height.saturating_sub(2),
            ..inner
        };
        (filter, rows)
    }
}

/// `lazyos menu remind` (`rofi-remind`): the window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Reminders",
            format!("{said} (systemd, date, jq, wl-clipboard)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Clock,
        );
        return Ok(3);
    }
    let mut screen = RemindScreen::new();
    let code = crate::run_screen(terminal, design, "Reminders", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remind_needed_tools() {
        assert!(NEEDED.contains(&"systemd-run"));
        assert!(NEEDED.contains(&"systemd-analyze"));
    }

    #[test]
    fn left_formats_seconds() {
        assert_eq!(RemindScreen::left(30), "in less than a minute");
        assert_eq!(RemindScreen::left(90), "in 1 min");
        assert_eq!(RemindScreen::left(3600), "in 1 h");
        assert_eq!(RemindScreen::left(7200), "in 2 h");
        assert_eq!(RemindScreen::left(86400), "in 1 days");
    }
}
