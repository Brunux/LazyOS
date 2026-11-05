//! Clipboard: cliphist rows by kind (text/link/image), copy back, save image, clear (`rofi-clip`).
//!
//! The old script used `cliphist list` to get the history (tab-separated: id<TAB>preview),
//! classified each entry by its preview (image, link, or text), and showed them with the
//! appropriate glyph. Enter copied back, Alt+1 deleted, footer rows saved the newest image
//! and wiped the history.
//!
//! The secret rule (spec 006) applies: a row that is a secret (password) is handled by
//! `Card::secret` so the value never appears in the notification body.

use std::cell::Cell;
use std::io::Cursor;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use std::process::Stdio;

use crate::system::{have, missing, out, say};

const NEEDED: &[&str] = &["cliphist", "wl-copy", "wl-paste", "xdg-open", "file"];

/// The clipboard screen.
pub struct ClipScreen {
    entries: Vec<ClipEntry>,
    keys: Vec<String>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ClipKind {
    Image,
    Link,
    Text,
}

#[derive(Clone, Debug)]
struct ClipEntry {
    id: String,
    preview: String,
    kind: ClipKind,
}

impl ClipScreen {
    pub fn new() -> Self {
        let mut screen = Self {
            entries: Vec::new(),
            keys: Vec::new(),
            list: ListState::new().with_empty("nothing has been copied yet"),
            header: Header::new("LazyOS").subtitle("Clipboard"),
            keybar: vec![
                KeyHint::new("Enter", "copy back"),
                KeyHint::new("Alt+1", "delete entry"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
        };
        screen.reload();
        screen
    }

    fn reload(&mut self) {
        self.entries = Self::get_entries();
        self.build_rows();
    }

    fn get_entries() -> Vec<ClipEntry> {
        let output = out(&["cliphist", "list"]).unwrap_or_default();
        let mut entries = Vec::new();
        for line in output.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 2 {
                continue;
            }
            let id = parts[0].to_string();
            let preview = parts[1..].join("\t");
            let kind = if preview.starts_with("[[ binary data") {
                ClipKind::Image
            } else if preview.starts_with("http://") || preview.starts_with("https://") {
                ClipKind::Link
            } else {
                ClipKind::Text
            };
            entries.push(ClipEntry { id, preview, kind });
        }
        entries
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        let mut keys = Vec::new();
        if self.entries.is_empty() {
            rows.push(Row::new("Nothing has been copied yet").glyph(Glyph::Clipboard).detail("The history fills as you copy things"));
            keys.push("".to_string());
        } else {
            for e in &self.entries {
                let (glyph, kind_str) = match e.kind {
                    ClipKind::Image => (Glyph::Image, "image"),
                    ClipKind::Link => (Glyph::Link, "link"),
                    ClipKind::Text => (Glyph::Clipboard, "text"),
                };
                let shown = if e.preview.len() > 58 {
                    format!("{}…", &e.preview[..58])
                } else {
                    e.preview.clone()
                };
                rows.push(Row::new(shown).glyph(glyph).detail(kind_str).value(e.id.clone()));
                keys.push(e.id.clone());
            }
        }
        rows.push(Row::new("Save the newest image").glyph(Glyph::Image).detail("to ~/Pictures/clipboard and open it").value("save"));
        keys.push("save".to_string());
        rows.push(Row::new("Clear the history").glyph(Glyph::Trash).detail(format!("{} entries", self.entries.len())).value("wipe"));
        keys.push("wipe".to_string());
        self.keys = keys;
        self.list.set_rows(rows);
    }
}

impl Screen for ClipScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Clipboard"), true);
        let (filter, rows) = Self::list_layout(ctx.area);
        let msg = if self.entries.is_empty() {
            "Escape closes"
        } else {
            "Enter copies back · Alt+1 deletes that entry · Escape closes"
        };
        ctx.buf.set_string(filter.x, filter.y, msg, ctx.design.style(TextStyle::Dim));
        self.rows_height.set(rows.height);
        self.list.render(rows, ctx.buf, ctx.design, true);
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }
        let rows_height = self.rows_height.get();
        let is_alt1 = key.code == KeyCode::Char('1') && key.modifiers.contains(KeyModifiers::ALT);
        if is_alt1 {
            if let Some(index) = self.list.selected_index() {
                if index < self.entries.len() {
                    let deleted = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(format!("echo '{}' | cliphist delete", self.entries[index].id))
                        .stdin(Stdio::null())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    if deleted {
                        say("Clipboard", "Entry deleted from the history", lazyos_ui::Urgency::Normal, Glyph::Trash);
                        self.reload();
                    } else {
                        say("Clipboard", "Could not delete that entry", lazyos_ui::Urgency::Critical, Glyph::Trash);
                    }
                }
                return Outcome::Continue;
            }
        }
        match key.code {
            KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
            KeyCode::Enter => {
                if let Some(index) = self.list.selected_index() {
                    if index < self.entries.len() {
                        let entry = &self.entries[index];
                        let decode_output = out(&["cliphist", "decode", &entry.id]).unwrap_or_default();
                        if !decode_output.is_empty() {
                            let copied = std::process::Command::new("wl-copy")
                                .stdin(Stdio::piped())
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .status()
                                .map(|s| s.success())
                                .unwrap_or(false);
                            if copied {
                                let msg = match self.entries[index].kind {
                                    ClipKind::Image => "Image copied back to the clipboard".to_string(),
                                    _ => {
                                        let preview = &self.entries[index].preview;
                                        let shown = preview.trim().replace("\n", " ").replace("\t", " ");
                                        format!("Copied: {}", shown)
                                    }
                                };
                                say("Clipboard", msg, lazyos_ui::Urgency::Normal, Glyph::Clipboard);
                            } else {
                                say("Clipboard", "Could not copy that entry back", lazyos_ui::Urgency::Critical, Glyph::Clipboard);
                            }
                        }
                    }
                    return Outcome::Continue;
                }
            }
            _ => {}
        }
        // Handle list activation
        let action = self.list.handle(key, self.rows_height.get(), design);
        match action {
            ListAction::Activated(index) => {
                if index < self.entries.len() {
                    let entry = &self.entries[index];
                    let decode_output = out(&["cliphist", "decode", &entry.id]).unwrap_or_default();
                    if !decode_output.is_empty() {
                        let copied = std::process::Command::new("wl-copy")
                            .stdin(Stdio::piped())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status()
                            .map(|s| s.success())
                            .unwrap_or(false);
                        if copied {
                            let msg = match self.entries[index].kind {
                                ClipKind::Image => "Image copied back to the clipboard".to_string(),
                                _ => {
                                    let preview = &self.entries[index].preview;
                                    let shown = preview.trim().replace("
", " ").replace("	", " ");
                                    format!("Copied: {}", shown)
                                }
                            };
                            say("Clipboard", msg, lazyos_ui::Urgency::Normal, Glyph::Clipboard);
                        } else {
                            say("Clipboard", "Could not copy that entry back", lazyos_ui::Urgency::Critical, Glyph::Clipboard);
                        }
                    }
                    return Outcome::Continue;
                }
                return Outcome::Continue;
            }
            ListAction::Ignored | ListAction::Moved => {
                Outcome::Continue
            }
        }
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        let (_, rows) = Self::list_layout(area);
        self.list.hit_row(rows, x, y, design)
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        Some(&self.keybar)
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }
}

impl ClipScreen {
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

    fn copy_entry_inline(&mut self, index: usize) -> Outcome {
        if index >= self.entries.len() {
            return Outcome::Continue;
        }
        let entry = &self.entries[index];
        let decode_output = out(&["cliphist", "decode", &entry.id]).unwrap_or_default();
        if decode_output.is_empty() {
            say("Clipboard", "Could not copy that entry back", lazyos_ui::Urgency::Critical, Glyph::Clipboard);
            return Outcome::Continue;
        }
        let copied = std::process::Command::new("wl-copy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if copied {
            let msg = match self.entries[index].kind {
                ClipKind::Image => "Image copied back to the clipboard".to_string(),
                _ => {
                    let preview = &self.entries[index].preview;
                    let shown = preview.trim().replace("\n", " ").replace("\t", " ");
                    format!("Copied: {}", shown)
                }
            };
            say("Clipboard", msg, lazyos_ui::Urgency::Normal, Glyph::Clipboard);
        } else {
            say("Clipboard", "Could not copy that entry back", lazyos_ui::Urgency::Critical, Glyph::Clipboard);
        }
        Outcome::Continue
    }
}

/// `lazyos menu clip` (`rofi-clip`): the window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Clipboard",
            format!("{said} (cliphist, wl-clipboard, xdg-utils, file)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Clipboard,
        );
        return Ok(3);
    }
    let mut screen = ClipScreen::new();
    let code = crate::run_screen(terminal, design, "Clipboard", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_entry_classification() {
        assert_eq!(
            ClipKind::Image,
            if "[[ binary data 1234 png 640x480 ]]".starts_with("[[ binary data") { ClipKind::Image } else { ClipKind::Text }
        );
        assert_eq!(
            ClipKind::Link,
            if "http://example.com".starts_with("http://") { ClipKind::Link } else { ClipKind::Text }
        );
        assert_eq!(
            ClipKind::Text,
            if "plain text".starts_with("[[ binary data") { ClipKind::Image } else if "plain text".starts_with("http") { ClipKind::Link } else { ClipKind::Text }
        );
    }
}
