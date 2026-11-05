//! Audio output picker (`bar-audio`): switch the default sink, mute, pavucontrol.
//!
//! The old script used `pactl` to list sinks, marked the default one, and offered
//! mute/unmute for output and microphone, plus "Volume per application" which
//! opens pavucontrol.

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
use std::cell::Cell;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, Input, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::system::{missing, out, say };

const NEEDED: &[&str] = &["pactl", "pavucontrol"];

/// The audio screen.
pub struct AudioScreen {
    sinks: Vec<Sink>,
    default_sink: String,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    keys: Vec<String>,
}

#[derive(Clone, Debug)]
struct Sink {
    name: String,
    description: String,
    is_default: bool,
}

impl AudioScreen {
    pub fn new() -> Self {
        let mut screen = Self {
            keys: Vec::new(),
            sinks: Vec::new(),
            default_sink: String::new(),
            list: ListState::new(),
            header: Header::new("LazyOS").subtitle("Audio output"),
            keybar: vec![
                KeyHint::new("Enter", "switch"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
        };
        screen.reload();
        screen
    }

    fn reload(&mut self) {
        self.default_sink = out(&["pactl", "get-default-sink"])
            .unwrap_or_default()
            .trim()
            .to_string();
        self.sinks = Self::get_sinks(&self.default_sink);
        self.build_rows();
    }

    fn get_sinks(default: &str) -> Vec<Sink> {
        let output = out(&["pactl", "list", "sinks"]).unwrap_or_default();
        let mut sinks = Vec::new();
        let mut name = String::new();
        for line in output.lines() {
            let line = line.trim();
            if line.starts_with("Name: ") {
                name = line["Name: ".len()..].to_string();
            } else if line.starts_with("Description: ") {
                let desc = line["Description: ".len()..].to_string();
                sinks.push(Sink {
                    name: name.clone(),
                    description: desc,
                    is_default: name == default,
                });
                name.clear();
            }
        }
        // Sort: default first
        sinks.sort_by(|a, b| b.is_default.cmp(&a.is_default));
        sinks
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        let mut keys = Vec::new();
        if self.sinks.is_empty() {
            rows.push(Row::new("No audio outputs").glyph(Glyph::Volume).detail(""));
            keys.push("".to_string());
        } else {
            for s in &self.sinks {
                let label = if s.is_default {
                    format!("* {}", s.description)
                } else {
                    format!("  {}", s.description)
                };
                let key = format!("sink:{}", s.name);
                keys.push(key.clone());
                rows.push(
                    Row::new(label)
                        .glyph(Glyph::Volume)
                        .detail(s.name.clone())
                        .value(key),
                );
            }
        }
        let key1 = "pavucontrol".to_string();
        keys.push(key1.clone());
        rows.push(
            Row::new("Volume per application")
                .glyph(Glyph::Sliders)
                .detail("pavucontrol: per-stream volume, ports, profiles")
                .value(key1),
        );
        let key2 = "mute-sink".to_string();
        keys.push(key2.clone());
        rows.push(
            Row::new("Mute / unmute output")
                .glyph(Glyph::Volume)
                .detail("toggle the default sink mute")
                .value(key2),
        );
        let key3 = "mute-source".to_string();
        keys.push(key3.clone());
        rows.push(
            Row::new("Mute / unmute microphone")
                .glyph(Glyph::Mic)
                .detail("toggle the default source mute")
                .value(key3),
        );
        self.keys = keys;
        self.list.set_rows(rows);
    }
}

impl Screen for AudioScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Audio output"), true);
        let (filter, rows) = super::pick::list_layout(ctx.area);
        ctx.buf.set_string(
            filter.x,
            filter.y,
            "Enter switches the default sink · footer rows for mute and pavucontrol",
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

impl AudioScreen {
    fn activate(&mut self, index: usize) -> Outcome {
        let rows = self.list.rows();
        if index >= rows.len() {
            return Outcome::Continue;
        }
        let key = self.keys.get(index).cloned().unwrap_or_default();
        if key.is_empty() || key == "-" {
            return Outcome::Continue;
        }
        if key.starts_with("sink:") {
            let name = &key[5..];
            let _ = std::process::Command::new("pactl")
                .args(["set-default-sink", name])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            // Move existing streams
            if let Ok(output) = std::process::Command::new("pactl")
                .args(["list", "short", "sink-inputs"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
            {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let id = line.split_whitespace().next();
                    if let Some(id) = id {
                        let _ = std::process::Command::new("pactl")
                            .args(["move-sink-input", id, name])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                    }
                }
            }
            self.reload();
            say(
                "Audio output",
                format!("Default sink: {}", name),
                lazyos_ui::Urgency::Normal,
                Glyph::Volume,
            );
        } else {
            match key.as_str() {
                "pavucontrol" => {
                    let _ = std::process::Command::new("setsid")
                        .arg("pavucontrol")
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                }
                "mute-sink" => {
                    let _ = std::process::Command::new("pactl")
                        .args(["set-sink-mute", "@DEFAULT_SINK@", "toggle"])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    say(
                        "Audio output",
                        "Sink mute toggled",
                        lazyos_ui::Urgency::Normal,
                        Glyph::Volume,
                    );
                }
                "mute-source" => {
                    let _ = std::process::Command::new("pactl")
                        .args(["set-source-mute", "@DEFAULT_SOURCE@", "toggle"])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    say(
                        "Audio output",
                        "Microphone mute toggled",
                        lazyos_ui::Urgency::Normal,
                        Glyph::Mic,
                    );
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

/// `lazyos menu audio` (`bar-audio`): the window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Audio output",
            format!("{said} (pactl, pavucontrol)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Volume,
        );
        return Ok(3);
    }
    let mut screen = AudioScreen::new();
    let code = crate::run_screen(terminal, design, "Audio output", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_needed_tools() {
        assert!(NEEDED.contains(&"pactl"));
        assert!(NEEDED.contains(&"pavucontrol"));
    }
}
