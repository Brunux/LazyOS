//! Notify history: structured JSONL history with dismiss/clear (`rofi-notify`).
//!
//! Notifications are logged to `$XDG_STATE_HOME/lazyos/notify/history.jsonl`
//! by `lazyos-notify`. Enter on a row copies the notification to the clipboard,
//! DND toggles quiet mode without suppressing critical system alerts, and
//! dismiss clears the log.

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
use std::cell::Cell;
use std::fs;
use std::io::BufRead;
use std::path::PathBuf;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::system::{missing, out, say};

const NEEDED: &[&str] = &["wl-copy"];

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

fn history_path() -> PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".local").join("state")
        });
    base.join("lazyos").join("notify").join("history.jsonl")
}

fn dnd_path() -> PathBuf {
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(runtime)
        .join("lazyos")
        .join("notify")
        .join("dnd")
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
        let path = history_path();
        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let mut notifications = Vec::new();
        let reader = std::io::BufReader::new(file);

        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                let id = json
                    .get("id")
                    .and_then(|v| {
                        if let Some(n) = v.as_u64() {
                            Some(n.to_string())
                        } else {
                            v.as_str().map(|s| s.to_string())
                        }
                    })
                    .unwrap_or_default();
                let app = json
                    .get("app_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let summary = json
                    .get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let body = json
                    .get("body")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                if !id.is_empty() || !summary.is_empty() {
                    notifications.push(Notification {
                        id,
                        app,
                        summary,
                        body,
                    });
                }
            }
        }

        // Most recent first
        notifications.reverse();
        notifications
    }

    fn get_dnd() -> bool {
        dnd_path().exists()
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
                    format!("{}\n{}", n.summary, n.body)
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
                return Outcome::Done(lazyos_ui::app::exit::OK);
            }
        } else {
            match key {
                "dnd" => {
                    let path = dnd_path();
                    if path.exists() {
                        let _ = fs::remove_file(&path);
                    } else {
                        if let Some(parent) = path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let _ = fs::write(&path, "1\n");
                    }
                    self.reload();
                }
                "restore" => {
                    if let Some(last) = self.notifications.first() {
                        let mut cmd = std::process::Command::new("notify-send");
                        if !last.app.is_empty() {
                            cmd.arg("-a").arg(&last.app);
                        }
                        cmd.arg(&last.summary);
                        if !last.body.is_empty() {
                            cmd.arg(&last.body);
                        }
                        let _ = cmd
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();
                        say(
                            "Notifications",
                            "Last notification restored",
                            lazyos_ui::Urgency::Normal,
                            Glyph::Bell,
                        );
                    }
                }
                "dismiss" => {
                    let path = history_path();
                    let _ = fs::write(&path, "");
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
            format!("{said} (wl-clipboard)"),
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
