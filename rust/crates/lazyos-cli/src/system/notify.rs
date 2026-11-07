//! Notify history: mako's history with dismiss/clear (`rofi-notify`).
//!
//! The old script used `makoctl history -j` to get the JSON history, parsed it
//! with `jq`, and showed each notification with its summary and body. The two
//! footer rows were dismiss-all and toggle DND. Enter on a row copied the
//! notification to the clipboard.

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
use std::cell::Cell;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::system::{missing, out, say};

const NEEDED: &[&str] = &["makoctl", "jq", "wl-copy"];

/// The notify screen.
pub struct NotifyScreen {
    notifications: Vec<Notification>,
    dnd: bool,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
}

#[derive(Clone, Debug)]
struct Notification {
    id: String,
    app: String,
    summary: String,
    body: String,
}

impl NotifyScreen {
    pub fn new() -> Self {
        let mut screen = Self {
            notifications: Vec::new(),
            dnd: false,
            list: ListState::new(),
            header: Header::new("LazyOS").subtitle("Notifications"),
            keybar: vec![KeyHint::new("Enter", "copy"), KeyHint::new("Esc", "close")],
            rows_height: Cell::new(20),
        };
        screen.reload();
        screen
    }

    fn reload(&mut self) {
        self.notifications = Self::get_notifications();
        self.dnd = Self::get_dnd();
        self.build_rows();
    }

    fn get_notifications() -> Vec<Notification> {
        let output = out(&["makoctl", "history", "-j"]).unwrap_or_default();
        let mut notifications = Vec::new();
        // Parse JSON with a simple approach - each notification is an object
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&output) {
            if let Some(arr) = json.as_array() {
                for item in arr {
                    let id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let app = item
                        .get("app_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let summary = item
                        .get("summary")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let body = item
                        .get("body")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !id.is_empty() {
                        notifications.push(Notification {
                            id,
                            app,
                            summary,
                            body,
                        });
                    }
                }
            }
        }
        notifications
    }

    fn get_dnd() -> bool {
        out(&["makoctl", "mode"])
            .map(|m| m.contains("do-not-disturb"))
            .unwrap_or(false)
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        if self.notifications.is_empty() {
            rows.push(
                Row::new("Nothing in the history yet")
                    .glyph(Glyph::Bell)
                    .detail("Notifications land here after they expire"),
            );
        } else {
            for n in &self.notifications {
                let body_short = if n.body.len() > 64 {
                    format!("{}…", &n.body[..64])
                } else {
                    n.body.clone()
                };
                let detail = format!("{} · {}", n.app, body_short);
                rows.push(
                    Row::new(n.summary.clone())
                        .glyph(Glyph::Bell)
                        .detail(detail)
                        .value(format!("id:{}", n.id)),
                );
            }
        }
        let dnd_label = if self.dnd {
            "Do not disturb · on"
        } else {
            "Do not disturb · off"
        };
        let dnd_glyph = if self.dnd {
            Glyph::BellOff
        } else {
            Glyph::Bell
        };
        let dnd_color = if self.dnd {
            lazyos_ui::ColorRole::Warn
        } else {
            lazyos_ui::ColorRole::Dim
        };
        rows.push(
            Row::new(dnd_label)
                .glyph(dnd_glyph)
                .detail("toggle do-not-disturb")
                .value("dnd"),
        );
        rows.push(
            Row::new("Restore the last one")
                .glyph(Glyph::Undo)
                .detail("back on screen")
                .value("restore"),
        );
        let count = self.notifications.len();
        rows.push(
            Row::new("Dismiss everything")
                .glyph(Glyph::Trash)
                .detail(format!("{} in the history", count))
                .value("dismiss"),
        );
        self.list.set_rows(rows);
    }
}

impl Screen for NotifyScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Notifications"), true);
        let (filter, rows) = super::pick::list_layout(ctx.area);
        let msg = if self.notifications.is_empty() {
            "Escape closes"
        } else {
            "Enter copies the notification · Escape closes"
        };
        ctx.buf
            .set_string(filter.x, filter.y, msg, ctx.design.style(TextStyle::Dim));
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

impl NotifyScreen {
    fn activate(&mut self, index: usize) -> Outcome {
        let rows = self.list.rows();
        let Some(row) = rows.get(index) else {
            return Outcome::Continue;
        };
        let Some(key) = row.value.as_deref() else {
            return Outcome::Continue;
        };
        if key.is_empty() || key == "-" {
            return Outcome::Continue;
        }
        if key.starts_with("id:") {
            let id = &key[3..];
            if let Some(n) = self.notifications.iter().find(|n| n.id == id) {
                let text = if n.body.is_empty() {
                    n.summary.clone()
                } else {
                    format!(
                        "{}
{}",
                        n.summary, n.body
                    )
                };
                let _ = std::process::Command::new("wl-copy")
                    .arg(&text)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                say(
                    "Notifications",
                    format!("Copied: {}", n.summary),
                    lazyos_ui::Urgency::Normal,
                    Glyph::Clipboard,
                );
            }
        } else {
            match key {
                "dnd" => {
                    let _ = std::process::Command::new("makoctl")
                        .args(["mode", "-t", "do-not-disturb"])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    self.reload();
                }
                "restore" => {
                    let _ = std::process::Command::new("makoctl")
                        .args(["restore"])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    say(
                        "Notifications",
                        "Last notification restored",
                        lazyos_ui::Urgency::Normal,
                        Glyph::Bell,
                    );
                }
                "dismiss" => {
                    let _ = std::process::Command::new("makoctl")
                        .args(["dismiss", "-a"])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    say(
                        "Notifications",
                        "Everything dismissed",
                        lazyos_ui::Urgency::Normal,
                        Glyph::Trash,
                    );
                    self.reload();
                }
                _ => {}
            }
        }
        Outcome::Continue
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

/// `lazyos menu notify` (`rofi-notify`): the window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Notifications",
            format!("{said} (makoctl, jq, wl-clipboard)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Bell,
        );
        return Ok(3);
    }
    let mut screen = NotifyScreen::new();
    let code = crate::run_screen(terminal, design, "Notifications", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_parsing() {
        // Compile-time check
    }
}
