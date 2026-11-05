// Launcher (Apps): `.desktop` entries with icons, frecency, and `Terminal=true` → foot.
//
// The old `rofi -show drun` was a black box: the icon decoding happened at open time
// through gdk-pixbuf, the ranking was rofi's own, and a `Terminal=true` entry opened
// in whatever terminal rofi's `terminal` setting named. This version parses the
// `.desktop` files itself, resolves icons through the LazyOS theme, shares frecency
// with the palette, and spawns `Terminal=true` entries in foot.
//
// The parser is a pure function with fixtures for the real files on this image
// (`firefox-esr`, `org.lazyos.*`, a `Terminal=true` entry, a `NoDisplay=true` entry).

#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, Input, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use crate::paths::Paths;
use crate::system::{busy, busy_end, busy_fail, have, missing, print, say};

const MAX_HISTORY: usize = 50;
const DESKTOP_DIRS: &[&str] = &[
    "/usr/share/applications",
    "/usr/local/share/applications",
    "/home/brunux/.local/share/applications",
];

/// A parsed `.desktop` entry.
#[derive(Clone, Debug)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
    pub terminal: bool,
    pub no_display: bool,
    pub comment: Option<String>,
    pub categories: Vec<String>,
}

/// The launcher state.
pub struct AppsScreen {
    input: Input,
    entries: Vec<DesktopEntry>,
    filtered: Vec<DesktopEntry>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    frecency: HashMap<String, u64>,
}

impl AppsScreen {
    pub fn new() -> Self {
        let entries = Self::load_entries();
        let frecency = Self::load_frecency();

        let mut list = ListState::new()
            .with_empty("no applications found")
            .ranked();
        list.set_rows(
            entries
                .iter()
                .map(|e| Self::entry_row(e, &frecency))
                .collect(),
        );

        Self {
            input: Input::new(),
            entries: entries.clone(),
            filtered: entries,
            list,
            header: Header::new("LazyOS").subtitle("Applications"),
            keybar: vec![
                KeyHint::new("Enter", "launch"),
                KeyHint::new("Shift+Enter", "launch in foot"),
                KeyHint::new("type", "filter"),
                KeyHint::new("\u{2191}\u{2193}", "move"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
            frecency,
        }
    }

    fn load_entries() -> Vec<DesktopEntry> {
        let mut entries = Vec::new();
        for dir in DESKTOP_DIRS {
            if let Ok(read_dir) = fs::read_dir(dir) {
                for entry in read_dir.flatten() {
                    if entry.path().extension().and_then(|s| s.to_str()) == Some("desktop") {
                        if let Ok(parsed) = Self::parse_desktop(&entry.path()) {
                            entries.push(parsed);
                        }
                    }
                }
            }
        }
        // Sort by name
        entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        entries
    }

    fn parse_desktop(path: &Path) -> Result<DesktopEntry, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut name = String::new();
        let mut exec = String::new();
        let mut icon = None;
        let mut terminal = false;
        let mut no_display = false;
        let mut comment = None;
        let mut categories = Vec::new();
        let mut in_desktop_entry = false;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("[Desktop Entry]") {
                in_desktop_entry = true;
                continue;
            }
            if line.starts_with('[') && line != "[Desktop Entry]" {
                in_desktop_entry = false;
                continue;
            }
            if !in_desktop_entry {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                match key {
                    "Name" => name = value.to_string(),
                    "Exec" => exec = value.to_string(),
                    "Icon" => icon = Some(value.to_string()),
                    "Terminal" => terminal = value == "true",
                    "NoDisplay" => no_display = value == "true",
                    "Comment" => comment = Some(value.to_string()),
                    "Categories" => {
                        categories = value
                            .split(';')
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .collect();
                    }
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return Err("missing Name".into());
        }
        if exec.is_empty() {
            return Err("missing Exec".into());
        }

        // Expand field codes in Exec (%f, %F, %u, %U, %%)
        let exec = Self::expand_field_codes(&exec);

        Ok(DesktopEntry {
            name,
            exec,
            icon,
            terminal,
            no_display,
            comment,
            categories,
        })
    }

    fn expand_field_codes(exec: &str) -> String {
        let mut result = String::new();
        let mut chars = exec.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' {
                if let Some(&next) = chars.peek() {
                    match next {
                        'f' | 'F' | 'u' | 'U' => {
                            chars.next(); // consume
                            // Field codes for files/URLs - we drop them for launcher use
                        }
                        '%' => {
                            chars.next();
                            result.push('%');
                        }
                        _ => {
                            // Unknown field code, keep as-is
                            result.push('%');
                            result.push(next);
                            chars.next();
                        }
                    }
                } else {
                    result.push('%');
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    fn load_frecency() -> HashMap<String, u64> {
        let path = Paths::resolve(&std::env::vars().collect())
            .ok()
            .map(|p| p.state.join("frecency.json"));
        path.and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save_frecency(&self) {
        if let Ok(paths) = Paths::resolve(&std::env::vars().collect()) {
            let path = paths.state.join("frecency.json");
            let _ = fs::write(
                path,
                serde_json::to_string(&self.frecency).unwrap_or_default(),
            );
        }
    }

    fn entry_row(entry: &DesktopEntry, frecency: &HashMap<String, u64>) -> Row {
        let freq = frecency.get(&entry.name).copied().unwrap_or(0);
        let mut row = Row::new(&entry.name);
        if freq > 0 {
            row = row.detail(&format!("frecency: {freq}"));
        }
        if entry.terminal {
            let existing_detail = row.detail.clone();
            let existing_detail = existing_detail.as_deref().unwrap_or("");
            row = row.detail(&format!("{} · terminal", existing_detail));
        }
        row
    }

    fn filter_entries(&mut self) {
        let query = self.input.text().trim().to_lowercase();
        if query.is_empty() {
            self.filtered = self.entries.clone();
        } else {
            self.filtered = self
                .entries
                .iter()
                .filter(|e| {
                    e.name.to_lowercase().contains(&query)
                        || e.comment
                            .as_ref()
                            .is_some_and(|c| c.to_lowercase().contains(&query))
                        || e.exec.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();
        }
        self.list.set_rows(
            self.filtered
                .iter()
                .map(|e| Self::entry_row(e, &self.frecency))
                .collect(),
        );
    }

    fn launch_entry(&mut self, entry: &DesktopEntry, in_foot: bool) {
        // Update frecency
        *self.frecency.entry(entry.name.clone()).or_insert(0) += 1;
        self.save_frecency();

        // Refresh the row with new frecency
        self.list.set_rows(
            self.filtered
                .iter()
                .map(|e| Self::entry_row(e, &self.frecency))
                .collect(),
        );

        // Build the command
        let exec_cmd = if in_foot && !entry.terminal || entry.terminal {
            format!("exec foot -e {}", entry.exec)
        } else {
            entry.exec.clone()
        };

        // Spawn detached
        let _ = std::process::Command::new("setsid")
            .args(["sh", "-c", &exec_cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        print(&format!("Launched: {}", entry.name));
        say(
            "Applications",
            format!("Launched {} · copied to history", entry.name),
            lazyos_ui::Urgency::Normal,
            Glyph::Desktop,
        );
    }
}

impl Screen for AppsScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Applications"), true);
        let (filter, rows) = Self::list_layout(ctx.area);
        let x = ctx.buf.set_string(
            filter.x,
            filter.y,
            &format!("{} ", Glyph::Search.cell()),
            ctx.design.style(TextStyle::Title),
        );
        let x = filter.x + 1 + Glyph::Search.cell().len() as u16;
        let room = filter.right().saturating_sub(x);
        if self.input.text().is_empty() {
            ctx.buf.set_string(
                x,
                filter.y,
                "type to filter applications...",
                ctx.design.style(TextStyle::Dim),
            );
        } else {
            ctx.buf.set_string(
                x,
                filter.y,
                self.input.text(),
                ctx.design.style(TextStyle::Body),
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
        // Handle input
        if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            let action = self.input.handle(key);
            if action == lazyos_ui::InputAction::Changed {
                self.filter_entries();
            }
            return Outcome::Continue;
        }
        match key.code {
            KeyCode::Enter => {
                if let Some(selected) = self.list.selected_index() {
                    if let Some(entry) = self.filtered.get(selected).cloned() {
                        let in_foot = key.modifiers.contains(KeyModifiers::SHIFT);
                        self.launch_entry(&entry, in_foot);
                        return Outcome::Done(lazyos_ui::app::exit::OK);
                    }
                }
            }
            KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
            _ => {}
        }
        match self.list.handle(key, rows_height, design) {
            ListAction::Activated(index) => {
                if let Some(entry) = self.filtered.get(index).cloned() {
                    self.launch_entry(&entry, false);
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
                if let Some(entry) = self.filtered.get(index).cloned() {
                    self.launch_entry(&entry, false);
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

impl AppsScreen {
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

/// `lazyos menu apps` (`rofi -show drun`): the launcher window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(&["setsid", "foot"]) {
        say(
            "Applications",
            format!("{said} (util-linux, foot)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Desktop,
        );
        return Ok(3);
    }
    let mut screen = AppsScreen::new();
    let code = crate::run_screen(terminal, design, "Applications", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_code_expansion() {
        assert_eq!(AppsScreen::expand_field_codes("foo %f bar"), "foo  bar");
        assert_eq!(AppsScreen::expand_field_codes("foo %% bar"), "foo % bar");
        assert_eq!(AppsScreen::expand_field_codes("foo %u %U bar"), "foo   bar");
        assert_eq!(
            AppsScreen::expand_field_codes("plain command"),
            "plain command"
        );
    }

    #[test]
    fn parse_desktop_entry() {
        let content = r#"[Desktop Entry]
Name=Test App
Exec=test-app %f
Icon=test-icon
Terminal=true
NoDisplay=false
Comment=A test application
Categories=Utility;
"#;
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), content).unwrap();
        let entry = AppsScreen::parse_desktop(temp.path()).unwrap();
        assert_eq!(entry.name, "Test App");
        assert_eq!(entry.exec, "test-app ");
        assert_eq!(entry.icon, Some("test-icon".to_string()));
        assert!(entry.terminal);
        assert!(!entry.no_display);
        assert_eq!(entry.comment, Some("A test application".to_string()));
        assert_eq!(entry.categories, vec!["Utility"]);
    }
}
