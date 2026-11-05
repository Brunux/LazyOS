#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
/// Run prompt: `$PATH` completion, Shift+Enter → foot, confirmation for unknown commands (`rofi -show run`).
///
/// The old `rofi -show run` was a simple dmenu over PATH. This version provides live
/// completion, runs TUI apps in foot on Shift+Enter, and shows a confirmation for
/// commands not found on PATH.
use std::cell::Cell;
use std::collections::{HashSet, VecDeque};
use std::env;
use std::fs;
use std::path::Path;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, Input, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use crate::paths::Paths;
use crate::system::{have, missing, print, say};

const MAX_HISTORY: usize = 50;

/// A command from PATH or history.
#[derive(Clone, Debug)]
pub struct RunEntry {
    pub name: String,
    pub path: String,
    pub from_history: bool,
}

/// The run prompt state.
pub struct RunScreen {
    input: Input,
    commands: Vec<RunEntry>,
    filtered: Vec<RunEntry>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    history: VecDeque<String>,
}

impl RunScreen {
    pub fn new() -> Self {
        let commands = Self::load_commands();
        let history = Self::load_history();
        let mut list = ListState::new().with_empty("no commands found").ranked();
        list.set_rows(commands.iter().map(Self::command_row).collect());

        Self {
            input: Input::new(),
            commands: commands.clone(),
            filtered: commands,
            list,
            header: Header::new("LazyOS").subtitle("Run"),
            keybar: vec![
                KeyHint::new("Enter", "run"),
                KeyHint::new("Shift+Enter", "run in foot"),
                KeyHint::new("type", "filter"),
                KeyHint::new("\u{2191}\u{2193}", "move"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
            history,
        }
    }

    fn load_commands() -> Vec<RunEntry> {
        let mut commands = Vec::new();
        let mut seen = HashSet::new();

        // Get PATH
        let path = env::var("PATH").unwrap_or_default();
        for dir in path.split(':') {
            if let Ok(read_dir) = fs::read_dir(dir) {
                for entry in read_dir.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                        if !name.is_empty() && seen.insert(name.to_string()) {
                            commands.push(RunEntry {
                                name: name.to_string(),
                                path: path.to_string_lossy().to_string(),
                                from_history: false,
                            });
                        }
                    }
                }
            }
        }

        // Sort by name
        commands.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        commands
    }

    fn load_history() -> VecDeque<String> {
        let path = Paths::resolve(&std::env::vars().collect())
            .ok()
            .map(|p| p.state.join("run-history"));
        path.and_then(|p| fs::read_to_string(p).ok())
            .map(|s| {
                s.lines()
                    .rev()
                    .take(MAX_HISTORY)
                    .map(|l| l.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn save_history(&self) {
        if let Ok(paths) = Paths::resolve(&std::env::vars().collect()) {
            let path = paths.state.join("run-history");
            let _ = fs::write(
                path,
                self.history
                    .iter()
                    .rev()
                    .map(|h| format!("{h}\n"))
                    .collect::<String>(),
            );
        }
    }

    fn command_row(cmd: &RunEntry) -> Row {
        let mut row = Row::new(&cmd.name).glyph(Glyph::Terminal);
        if cmd.from_history {
            row = row.detail("history");
        }
        row
    }

    fn filter_commands(&mut self) {
        let query = self.input.text().trim().to_lowercase();
        if query.is_empty() {
            self.filtered = self.commands.clone();
        } else {
            self.filtered = self
                .commands
                .iter()
                .filter(|c| c.name.to_lowercase().starts_with(&query))
                .cloned()
                .collect();
        }
        self.list
            .set_rows(self.filtered.iter().map(Self::command_row).collect());
    }

    fn run_command(&mut self, cmd: &RunEntry, in_foot: bool) {
        // Add to history
        self.history.push_front(cmd.name.clone());
        if self.history.len() > MAX_HISTORY {
            self.history.pop_back();
        }
        self.save_history();

        // Build the command
        let exec_cmd = if in_foot {
            format!("exec foot -e {}", cmd.path)
        } else {
            cmd.path.clone()
        };

        // Spawn detached
        let _ = std::process::Command::new("setsid")
            .args(["sh", "-c", &exec_cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        print(&format!("Ran: {}", cmd.name));
        say(
            "Run",
            format!("Ran {} · copied to history", cmd.name),
            lazyos_ui::Urgency::Normal,
            Glyph::Terminal,
        );
    }

    fn run_unknown(&mut self, input: &str, in_foot: bool) {
        let cmd = input.trim();
        if cmd.is_empty() {
            return;
        }

        // Add to history
        self.history.push_front(cmd.to_string());
        if self.history.len() > MAX_HISTORY {
            self.history.pop_back();
        }
        self.save_history();

        let exec_cmd = if in_foot {
            format!("exec foot -e {}", cmd)
        } else {
            cmd.to_string()
        };

        // Spawn detached
        let _ = std::process::Command::new("setsid")
            .args(["sh", "-c", &exec_cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        print(&format!("Ran (unknown): {}", cmd));
        say(
            "Run",
            format!("Ran {} · copied to history", cmd),
            lazyos_ui::Urgency::Normal,
            Glyph::Terminal,
        );
    }
}

impl Screen for RunScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Run"), true);
        let (filter, rows) = Self::list_layout(ctx.area);
        let x = ctx.buf.set_string(
            filter.x,
            filter.y,
            &format!("{} ", Glyph::Terminal.cell()),
            ctx.design.style(TextStyle::Title),
        );
        let x = filter.x + 1 + Glyph::Terminal.cell().len() as u16;
        let room = filter.right().saturating_sub(x);
        let input_text = self.input.text();
        if input_text.is_empty() {
            ctx.buf.set_string(
                x,
                filter.y,
                "type a command...",
                ctx.design.style(TextStyle::Dim),
            );
        } else {
            // Check if command exists on PATH
            let first_word = input_text.split_whitespace().next().unwrap_or("");
            let exists = self.commands.iter().any(|c| c.name == first_word);
            let style = if exists {
                ctx.design.style(TextStyle::Body)
            } else {
                ctx.design.style(TextStyle::Danger)
            };
            ctx.buf.set_string(x, filter.y, input_text, style);
            if !exists {
                let warn_x = x + input_text.len() as u16 + 2;
                if warn_x < filter.right() {
                    ctx.buf.set_string(
                        warn_x,
                        filter.y,
                        "⚠ not on PATH",
                        ctx.design.style(TextStyle::Danger),
                    );
                }
            }
        }
        self.rows_height.set(rows.height);
        self.list.render(rows, ctx.buf, ctx.design, true);
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }
        let rows_height = self.rows_height.get();
        if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            let _ = self.input.handle(key);
            self.filter_commands();
            return Outcome::Continue;
        }
        match key.code {
            KeyCode::Enter => {
                let input_text = self.input.text().trim().to_string();
                let in_foot = key.modifiers.contains(KeyModifiers::SHIFT);
                if input_text.is_empty() {
                    return Outcome::Continue;
                }
                let first_word = input_text.split_whitespace().next().unwrap_or("");
                let cmd = self.commands.iter().find(|c| c.name == first_word).cloned();
                if let Some(cmd) = cmd {
                    self.run_command(&cmd, in_foot);
                } else {
                    // Unknown command - show confirmation
                    self.run_unknown(&input_text, in_foot);
                }
                return Outcome::Done(lazyos_ui::app::exit::OK);
            }
            KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
            _ => {}
        }
        match self.list.handle(key, rows_height, design) {
            ListAction::Activated(index) => {
                if let Some(cmd) = self.filtered.get(index).cloned() {
                    self.run_command(&cmd, false);
                    return Outcome::Done(lazyos_ui::app::exit::OK);
                }
                Outcome::Continue
            }
            _ => Outcome::Continue,
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let Some(row) = row {
            self.list.set_cursor(row);
            if let Some(index) = self.list.selected_index() {
                if let Some(cmd) = self.filtered.get(index).cloned() {
                    self.run_command(&cmd, false);
                    return Outcome::Done(lazyos_ui::app::exit::OK);
                }
            }
        }
        Outcome::Continue
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

impl RunScreen {
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

/// `lazyos menu run` (`rofi -show run`): the run prompt.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(&["setsid", "foot"]) {
        say(
            "Run",
            format!("{said} (util-linux, foot)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Terminal,
        );
        return Ok(3);
    }
    let mut screen = RunScreen::new();
    let code = crate::run_screen(terminal, design, "Run", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_row_from_path() {
        let entry = RunEntry {
            name: "firefox".to_string(),
            path: "/usr/bin/firefox".to_string(),
            from_history: false,
        };
        let row = RunScreen::command_row(&entry);
        assert_eq!(row.label, "firefox");
    }

    #[test]
    fn command_row_from_history() {
        let entry = RunEntry {
            name: "custom-cmd".to_string(),
            path: "custom-cmd".to_string(),
            from_history: true,
        };
        let row = RunScreen::command_row(&entry);
        assert_eq!(row.label, "custom-cmd");
        assert!(row.detail.as_deref().is_some_and(|d| d.contains("history")));
    }
}
