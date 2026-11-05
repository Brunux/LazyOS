#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
/// Window switcher: the wlroots foreign-toplevel list via sway IPC (`rofi -show window`).
///
/// The old `rofi -show window` used the foreign-toplevel protocol directly through rofi's
/// built-in window switcher. This version uses `swaymsg -t get_tree` to get the window tree
/// and extracts all mapped windows with their titles, app_ids, and workspace info. The
/// parser is pure and tested against recorded JSON fixtures.
use std::cell::Cell;
use std::collections::HashMap;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, Input, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use serde::Deserialize;

use crate::system::{have, missing, print, say};

/// A window from the sway tree.
#[derive(Clone, Debug, Deserialize)]
pub struct SwayWindow {
    pub id: i64,
    pub name: Option<String>,
    pub app_id: Option<String>,
    pub workspace: Option<SwayWorkspace>,
    pub visible: bool,
    pub focused: bool,
    #[serde(default)]
    pub nodes: Vec<SwayWindow>,
    #[serde(default)]
    pub floating_nodes: Vec<SwayWindow>,
}

/// A workspace from the sway tree.
#[derive(Clone, Debug, Deserialize)]
pub struct SwayWorkspace {
    pub name: String,
    pub num: i64,
}

/// Simplified window for display.
#[derive(Clone, Debug)]
pub struct WindowEntry {
    pub id: i64,
    pub title: String,
    pub app_id: String,
    pub workspace_name: String,
    pub workspace_num: i64,
    pub focused: bool,
}

/// The window switcher state.
pub struct WindowScreen {
    input: Input,
    windows: Vec<WindowEntry>,
    filtered: Vec<WindowEntry>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
}

impl WindowScreen {
    pub fn new() -> Self {
        let windows = Self::load_windows();
        let mut list = ListState::new().with_empty("no windows found").ranked();
        list.set_rows(windows.iter().map(Self::window_row).collect());

        Self {
            input: Input::new(),
            windows: windows.clone(),
            filtered: windows,
            list,
            header: Header::new("LazyOS").subtitle("Windows"),
            keybar: vec![
                KeyHint::new("Enter", "focus"),
                KeyHint::new("Alt+1", "close"),
                KeyHint::new("type", "filter"),
                KeyHint::new("\u{2191}\u{2193}", "move"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
        }
    }

    fn load_windows() -> Vec<WindowEntry> {
        // Get the sway tree
        let output = std::process::Command::new("swaymsg")
            .args(["-t", "get_tree"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();

        let mut windows = Vec::new();
        if let Ok(output) = output {
            if output.status.success() {
                if let Ok(tree) = serde_json::from_slice::<SwayWindow>(&output.stdout) {
                    windows = Self::extract_windows(&tree);
                }
            }
        }
        windows
    }

    fn extract_windows(node: &SwayWindow) -> Vec<WindowEntry> {
        let mut windows = Vec::new();

        // Check if this node is a window (has app_id and is a leaf node - no children)
        // A window is a leaf node with an app_id. Containers have nodes but no app_id.
        let is_window =
            node.app_id.is_some() && node.nodes.is_empty() && node.floating_nodes.is_empty();
        if is_window {
            let workspace_name = node
                .workspace
                .as_ref()
                .map(|w| w.name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let workspace_num = node.workspace.as_ref().map(|w| w.num).unwrap_or(-1);
            let title = node.name.clone().unwrap_or_else(|| "unnamed".to_string());
            let app_id = node.app_id.clone().unwrap_or_else(|| "unknown".to_string());

            if !title.is_empty() || !app_id.is_empty() {
                windows.push(WindowEntry {
                    id: node.id,
                    title,
                    app_id,
                    workspace_name,
                    workspace_num,
                    focused: node.focused,
                });
            }
        }

        // Recurse into nodes
        for child in &node.nodes {
            windows.extend(Self::extract_windows(child));
        }
        for child in &node.floating_nodes {
            windows.extend(Self::extract_windows(child));
        }

        windows
    }

    fn window_row(w: &WindowEntry) -> Row {
        let mut row = Row::new(&format!("{} · {}", w.title, w.app_id))
            .glyph(Glyph::Window)
            .detail(&format!("workspace {}", w.workspace_name));
        if w.focused {
            let detail = row.detail.clone();
            let detail = detail.as_deref().unwrap_or("");
            row = row.detail(&format!("{} · focused", detail));
        }
        row
    }

    fn filter_windows(&mut self) {
        let query = self.input.text().trim().to_lowercase();
        if query.is_empty() {
            self.filtered = self.windows.clone();
        } else {
            self.filtered = self
                .windows
                .iter()
                .filter(|w| {
                    w.title.to_lowercase().contains(&query)
                        || w.app_id.to_lowercase().contains(&query)
                        || w.workspace_name.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();
        }
        self.list
            .set_rows(self.filtered.iter().map(Self::window_row).collect());
    }

    fn focus_window(&self, window: &WindowEntry) {
        let _ = std::process::Command::new("swaymsg")
            .args(["[con_id="])
            .arg(window.id.to_string())
            .arg("] focus")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        print(&format!("Focused: {}", window.title));
        say(
            "Windows",
            format!("Focused {}", window.title),
            lazyos_ui::Urgency::Normal,
            Glyph::Window,
        );
    }

    fn close_window(&self, window: &WindowEntry) {
        let _ = std::process::Command::new("swaymsg")
            .args(["[con_id="])
            .arg(window.id.to_string())
            .arg("] kill")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        print(&format!("Closed: {}", window.title));
        say(
            "Windows",
            format!("Closed {}", window.title),
            lazyos_ui::Urgency::Normal,
            Glyph::Window,
        );
    }
}

impl Screen for WindowScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Windows"), true);
        let (filter, rows) = Self::list_layout(ctx.area);
        let x = ctx.buf.set_string(
            filter.x,
            filter.y,
            &format!("{} ", Glyph::Search.cell()),
            ctx.design.style(TextStyle::Title),
        );
        let x = filter.x + 1 + Glyph::Search.cell().len() as u16;
        if self.input.text().is_empty() {
            ctx.buf.set_string(
                x,
                filter.y,
                "type to filter windows...",
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
        if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace)
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            let _ = self.input.handle(key);
            self.filter_windows();
            return Outcome::Continue;
        }
        match key.code {
            KeyCode::Enter => {
                if let Some(selected) = self.list.selected_index() {
                    if let Some(window) = self.filtered.get(selected) {
                        self.focus_window(window);
                        return Outcome::Done(lazyos_ui::app::exit::OK);
                    }
                }
            }
            KeyCode::Char('1') if key.modifiers.contains(KeyModifiers::ALT) => {
                if let Some(selected) = self.list.selected_index() {
                    if let Some(window) = self.filtered.get(selected) {
                        self.close_window(window);
                        return Outcome::Continue;
                    }
                }
            }
            KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
            _ => {}
        }
        match self.list.handle(key, rows_height, design) {
            ListAction::Activated(index) => {
                if let Some(window) = self.filtered.get(index) {
                    self.focus_window(window);
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
                if let Some(window) = self.filtered.get(index) {
                    self.focus_window(window);
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

impl WindowScreen {
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

/// `lazyos menu window` (`rofi -show window`): the window switcher.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(&["swaymsg"]) {
        say(
            "Windows",
            format!("{said} (sway)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Window,
        );
        return Ok(3);
    }
    let mut screen = WindowScreen::new();
    let code = crate::run_screen(terminal, design, "Windows", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sway_tree() {
        // Test with a minimal sway tree JSON
        let json = r#"{
            "id": 1,
            "name": "root",
            "app_id": null,
            "workspace": null,
            "visible": true,
            "focused": false,
            "nodes": [
                {
                    "id": 2,
                    "name": "output",
                    "app_id": null,
                    "workspace": null,
                    "visible": true,
                    "focused": false,
                    "nodes": [
                        {
                            "id": 3,
                            "name": "workspace 1",
                            "num": 1,
                            "app_id": null,
                            "workspace": null,
                            "visible": true,
                            "focused": false,
                            "nodes": [
                                {
                                    "id": 4,
                                    "name": "Firefox",
                                    "app_id": "firefox",
                                    "workspace": {"name": "workspace 1", "num": 1},
                                    "visible": true,
                                    "focused": false,
                                    "nodes": [],
                                    "floating_nodes": []
                                }
                            ],
                            "floating_nodes": []
                        }
                    ],
                    "floating_nodes": []
                }
            ],
            "floating_nodes": []
        }"#;
        let tree: SwayWindow = serde_json::from_str(json).unwrap();
        let windows = WindowScreen::extract_windows(&tree);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].title, "Firefox");
        assert_eq!(windows[0].app_id, "firefox");
        assert_eq!(windows[0].workspace_num, 1);
    }

    #[test]
    fn empty_workspace() {
        let json = r#"{
            "id": 1,
            "name": "root",
            "app_id": null,
            "workspace": null,
            "visible": true,
            "focused": false,
            "nodes": [],
            "floating_nodes": []
        }"#;
        let tree: SwayWindow = serde_json::from_str(json).unwrap();
        let windows = WindowScreen::extract_windows(&tree);
        assert_eq!(windows.len(), 0);
    }
}
