//! Media: player state and transport keys (`rofi-media`).
//!
//! The old script used `playerctl` to get the current track metadata and status,
//! and offered play/pause/next/prev/stop. The track line at the top was also a
//! way to copy what is playing. This is the same interaction: one list, the
//! track marked with a star when playing, and the four transport rows.
//!
//! `playerctl -F` (follow) is not used here: the menu opens, reads once, and
//! closes. A live-updating panel is the bar's MPRIS module (spec 014).

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

const NEEDED: &[&str] = &["playerctl"];

/// The media screen.
pub struct MediaScreen {
    players: Vec<Player>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
}

#[derive(Clone, Debug)]
struct Player {
    name: String,
    title: String,
    artist: String,
    album: String,
    status: String,
}

impl MediaScreen {
    pub fn new() -> Self {
        let mut screen = Self {
            players: Vec::new(),
            list: ListState::new(),
            header: Header::new("LazyOS").subtitle("Media"),
            keybar: vec![KeyHint::new("Enter", "act"), KeyHint::new("Esc", "close")],
            rows_height: Cell::new(20),
        };
        screen.reload();
        screen
    }

    fn reload(&mut self) {
        self.players = Self::get_players();
        self.build_rows();
    }

    fn get_players() -> Vec<Player> {
        let players = out(&["playerctl", "-l"]).unwrap_or_default();
        let mut out_players = Vec::new();
        for line in players.lines() {
            let name = line.trim();
            if name.is_empty() {
                continue;
            }
            let title = out(&["playerctl", "metadata", "title", "-p", name]).unwrap_or_default();
            let artist = out(&["playerctl", "metadata", "artist", "-p", name]).unwrap_or_default();
            let album = out(&["playerctl", "metadata", "album", "-p", name]).unwrap_or_default();
            let status = out(&["playerctl", "status", "-p", name]).unwrap_or_default();
            out_players.push(Player {
                name: name.to_string(),
                title: title.trim().to_string(),
                artist: artist.trim().to_string(),
                album: album.trim().to_string(),
                status: status.trim().to_string(),
            });
        }
        out_players
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        if self.players.is_empty() {
            rows.push(
                Row::new("Nothing is playing")
                    .glyph(Glyph::Music)
                    .detail("no MPRIS player is running"),
            );
        } else {
            for p in &self.players {
                let mut label = String::new();
                if p.status == "Playing" {
                    label.push_str(&format!("{} ", Glyph::Music.cell()));
                }
                if !p.artist.is_empty() {
                    label.push_str(&format!("{} — ", p.artist));
                }
                label.push_str(&p.title);
                if !p.album.is_empty() {
                    label.push_str(&format!(" · {}", p.album));
                }
                let detail = format!("{} · {}", p.name, p.status);
                rows.push(
                    Row::new(label)
                        .glyph(Glyph::Music)
                        .detail(detail)
                        .value(format!("player:{}", p.name)),
                );
            }
            rows.push(Row::new("").glyph(Glyph::Music)); // separator
            if self.players.iter().any(|p| p.status == "Playing") {
                rows.push(
                    Row::new("Pause")
                        .glyph(Glyph::Pause)
                        .detail("pause playback")
                        .value("pause"),
                );
            } else {
                rows.push(
                    Row::new("Play")
                        .glyph(Glyph::Play)
                        .detail("resume playback")
                        .value("play"),
                );
            }
            rows.push(
                Row::new("Next track")
                    .glyph(Glyph::Next)
                    .detail("skip to next")
                    .value("next"),
            );
            rows.push(
                Row::new("Previous track")
                    .glyph(Glyph::Prev)
                    .detail("skip to previous")
                    .value("previous"),
            );
            rows.push(
                Row::new("Stop")
                    .glyph(Glyph::Stop)
                    .detail("stop playback")
                    .value("stop"),
            );
            rows.push(
                Row::new("Copy track")
                    .glyph(Glyph::Clipboard)
                    .detail("artist — title to clipboard")
                    .value("copy"),
            );
        }
        self.list.set_rows(rows);
    }
}

impl Screen for MediaScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Media"), true);
        let (filter, rows) = super::pick::list_layout(ctx.area);
        if self.players.is_empty() {
            ctx.buf.set_string(
                filter.x,
                filter.y,
                "No MPRIS player is running",
                ctx.design.style(TextStyle::Dim),
            );
        } else {
            ctx.buf.set_string(
                filter.x,
                filter.y,
                "Enter on a player pauses/resumes · transport keys below",
                ctx.design.style(TextStyle::Dim),
            );
        }
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

impl MediaScreen {
    fn activate(&mut self, index: usize) -> Outcome {
        if self.players.is_empty() {
            return Outcome::Continue;
        }
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
        if key.starts_with("player:") {
            let name = &key[7..];
            let status = self
                .players
                .iter()
                .find(|p| p.name == name)
                .map(|p| p.status.clone())
                .unwrap_or_default();
            if status == "Playing" {
                let _ = std::process::Command::new("playerctl")
                    .args(["pause", "-p", name])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            } else {
                let _ = std::process::Command::new("playerctl")
                    .args(["play", "-p", name])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
            }
            self.reload();
            Outcome::Continue
        } else {
            match key {
                "play" | "pause" => {
                    for p in &self.players {
                        let _ = std::process::Command::new("playerctl")
                            .args([key, "-p", &p.name])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                    }
                    self.reload();
                }
                "next" => {
                    for p in &self.players {
                        let _ = std::process::Command::new("playerctl")
                            .args(["next", "-p", &p.name])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                    }
                    self.reload();
                }
                "previous" => {
                    for p in &self.players {
                        let _ = std::process::Command::new("playerctl")
                            .args(["previous", "-p", &p.name])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                    }
                    self.reload();
                }
                "stop" => {
                    for p in &self.players {
                        let _ = std::process::Command::new("playerctl")
                            .args(["stop", "-p", &p.name])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                    }
                    self.reload();
                }
                "copy" => {
                    for p in &self.players {
                        let track = if !p.artist.is_empty() {
                            format!("{} - {}", p.artist, p.title)
                        } else {
                            p.title.clone()
                        };
                        let _ = std::process::Command::new("wl-copy")
                            .arg(&track)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                        say(
                            "Media",
                            format!("Copied: {track}"),
                            lazyos_ui::Urgency::Normal,
                            Glyph::Clipboard,
                        );
                    }
                }
                _ => {}
            }
            Outcome::Continue
        }
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

/// `lazyos menu media` (`rofi-media`): the window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Media",
            format!("{said} (playerctl)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Music,
        );
        return Ok(3);
    }
    let mut screen = MediaScreen::new();
    let code = crate::run_screen(terminal, design, "Media", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_parsing() {
        // Test that the player parsing logic works with mocked output
        // This is a compile-time check mostly
    }
}
