#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
/// Package search and install: `apt-cache search` + `apt-get install` (`rofi-pkg`).
///
/// The old `rofi-pkg` used rofi's free mode for the query, then `apt-cache search`
/// and showed results with an install action. This version uses the same flow with
/// a task window for the install (the not-stoppable busy card).
use std::cell::Cell;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, Input, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use crate::system::{busy, busy_end, busy_fail, have, missing, print, say, sudo};

/// A package entry.
#[derive(Clone, Debug)]
pub struct PackageEntry {
    pub name: String,
    pub description: String,
    pub installed: bool,
}

/// The package screen state.
pub struct PkgScreen {
    input: Input,
    packages: Vec<PackageEntry>,
    filtered: Vec<PackageEntry>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    results_keyhints: Vec<KeyHint>,
    installing_keyhints: Vec<KeyHint>,
    stage: PkgStage,
    query: String,
}

/// Stage of the package screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PkgStage {
    Query,
    Results,
    Installing,
}

impl PkgScreen {
    pub fn new() -> Self {
        let mut list = ListState::new()
            .with_empty("type a search term and press Enter")
            .ranked();

        Self {
            input: Input::new(),
            packages: Vec::new(),
            filtered: Vec::new(),
            list,
            header: Header::new("LazyOS").subtitle("Packages"),
            keybar: vec![
                KeyHint::new("Enter", "search / install"),
                KeyHint::new("type", "query"),
                KeyHint::new("\u{2191}\u{2193}", "move"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
            stage: PkgStage::Query,
            query: String::new(),
            results_keyhints: vec![
                KeyHint::new("Enter", "install"),
                KeyHint::new("type", "filter"),
                KeyHint::new("Esc", "back"),
            ],
            installing_keyhints: vec![KeyHint::new("Esc", "back to results")],
        }
    }

    fn load_installed() -> std::collections::HashSet<String> {
        let mut installed = std::collections::HashSet::new();
        let output = std::process::Command::new("dpkg-query")
            .args(["-W", "-f=${db:Status-Abbrev}${Package}\n"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.starts_with("ii") {
                        installed.insert(line[2..].to_string());
                    }
                }
            }
        }
        installed
    }

    fn search(&mut self, query: &str) {
        self.query = query.to_string();
        self.stage = PkgStage::Results;

        let output = std::process::Command::new("apt-cache")
            .args(["search", "--", query])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();

        let installed = Self::load_installed();
        let mut packages = Vec::new();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().take(40) {
                    if let Some(dash_pos) = line.find(" - ") {
                        let name = line[..dash_pos].trim().to_string();
                        let desc = line[dash_pos + 3..].chars().take(72).collect::<String>();
                        packages.push(PackageEntry {
                            installed: installed.contains(&name),
                            name,
                            description: desc,
                        });
                    } else if !line.is_empty() {
                        packages.push(PackageEntry {
                            installed: installed.contains(line.trim()),
                            name: line.trim().to_string(),
                            description: String::new(),
                        });
                    }
                }
            }
        }

        self.packages = packages;
        self.filtered = self.packages.clone();
        self.list
            .set_rows(self.filtered.iter().map(Self::package_row).collect());
    }

    fn package_row(pkg: &PackageEntry) -> Row {
        let mut row = Row::new(&pkg.name).glyph(Glyph::Box);
        let mut detail = pkg.description.clone();
        if pkg.installed {
            detail = format!("{} · installed", detail);
        }
        row = row.detail(&detail);
        row
    }

    fn install(&mut self, name: &str) -> Result<(), String> {
        self.stage = PkgStage::Installing;

        // Start busy card
        let _ = busy(
            "pkg",
            "Packages",
            &format!("Installing {}", name),
            "package-x-generic",
        );

        let result = sudo(&[
            "apt-get",
            "install",
            "-y",
            "-o",
            "Dpkg::Options::=--force-confdef",
            "-o",
            "Dpkg::Options::=--force-confold",
            name,
        ]);

        match result {
            Ok(_) => {
                let _ = busy_end("pkg", "Packages", &format!("Installed {}", name));
                say(
                    "Packages",
                    format!("Installed {} · lists updated", name),
                    lazyos_ui::Urgency::Normal,
                    Glyph::Box,
                );
                Ok(())
            }
            Err(e) => {
                let _ = busy_fail("pkg", "Packages", &e, "dialog-error");
                Err(e)
            }
        }
    }

    fn update_lists(&mut self) -> Result<(), String> {
        self.stage = PkgStage::Installing;
        let _ = busy(
            "pkg",
            "Packages",
            "Updating package lists",
            "package-x-generic",
        );

        let result = sudo(&["apt-get", "update"]);

        match result {
            Ok(_) => {
                let _ = busy_end("pkg", "Packages", "Package lists updated");
                // Re-search
                let query = self.query.clone();
                self.search(&query);
                Ok(())
            }
            Err(e) => {
                let _ = busy_fail("pkg", "Packages", &e, "dialog-error");
                Err(e)
            }
        }
    }
}

impl Screen for PkgScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Packages"), true);

        match self.stage {
            PkgStage::Query => {
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
                        "search packages...",
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
            PkgStage::Results => {
                let (filter, rows) = Self::list_layout(ctx.area);
                let x = ctx.buf.set_string(
                    filter.x,
                    filter.y,
                    &format!("{} ", Glyph::Search.cell()),
                    ctx.design.style(TextStyle::Title),
                );
                let x = filter.x + 1 + Glyph::Search.cell().len() as u16;
                let msg = if self.input.text().is_empty() {
                    format!("results for \"{}\" · Enter installs", self.query)
                } else {
                    format!(
                        "results for \"{}\" (filter: {}) · Enter installs",
                        self.query,
                        self.input.text()
                    )
                };
                ctx.buf
                    .set_string(x, filter.y, &msg, ctx.design.style(TextStyle::Dim));
                self.rows_height.set(rows.height);
                self.list.render(rows, ctx.buf, ctx.design, true);
            }
            PkgStage::Installing => {
                let inner = Rect {
                    x: ctx.area.x + 2,
                    y: ctx.area.y + 1,
                    width: ctx.area.width.saturating_sub(4),
                    height: ctx.area.height.saturating_sub(2),
                };
                ctx.buf.set_string(
                    inner.x,
                    inner.y,
                    "Installing...",
                    ctx.design.style(TextStyle::Title),
                );
            }
        }
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }

        match self.stage {
            PkgStage::Query => {
                if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace)
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    let _ = self.input.handle(key);
                    return Outcome::Continue;
                }
                match key.code {
                    KeyCode::Enter => {
                        let query = self.input.text().trim().to_string();
                        if !query.is_empty() {
                            self.search(&query);
                        }
                    }
                    KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
                    _ => {}
                }
                Outcome::Continue
            }
            PkgStage::Results => {
                let rows_height = self.rows_height.get();
                if matches!(key.code, KeyCode::Char(_) | KeyCode::Backspace)
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                {
                    let _ = self.input.handle(key);
                    // Re-filter locally
                    let query = self.input.text().trim().to_lowercase();
                    if query.is_empty() {
                        self.filtered = self.packages.clone();
                    } else {
                        self.filtered = self
                            .packages
                            .iter()
                            .filter(|p| {
                                p.name.to_lowercase().contains(&query)
                                    || p.description.to_lowercase().contains(&query)
                            })
                            .cloned()
                            .collect();
                    }
                    self.list
                        .set_rows(self.filtered.iter().map(Self::package_row).collect());
                    return Outcome::Continue;
                }
                match key.code {
                    KeyCode::Enter => {
                        if let Some(selected) = self.list.selected_index() {
                            if let Some(pkg) = self.filtered.get(selected).cloned() {
                                let _ = self.install(&pkg.name);
                                self.stage = PkgStage::Results;
                                // Refresh installed status
                                return Outcome::Continue;
                            }
                        }
                    }
                    KeyCode::Esc => {
                        self.stage = PkgStage::Query;
                        self.input = Input::new();
                        return Outcome::Continue;
                    }
                    _ => {}
                }
                match self.list.handle(key, rows_height, design) {
                    ListAction::Activated(index) => {
                        if let Some(pkg) = self.filtered.get(index).cloned() {
                            let _ = self.install(&pkg.name);
                            self.stage = PkgStage::Results;
                        }
                        Outcome::Continue
                    }
                    _ => Outcome::Continue,
                }
            }
            PkgStage::Installing => match key.code {
                KeyCode::Esc => {
                    self.stage = PkgStage::Results;
                    Outcome::Continue
                }
                _ => Outcome::Continue,
            },
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let Some(row) = row {
            self.list.set_cursor(row);
            if let Some(index) = self.list.selected_index() {
                if let Some(pkg) = self.filtered.get(index).cloned() {
                    let _ = self.install(&pkg.name);
                    self.stage = PkgStage::Results;
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
        match self.stage {
            PkgStage::Query => Some(&self.keybar),
            PkgStage::Results => Some(&self.results_keyhints),
            PkgStage::Installing => Some(&self.installing_keyhints),
        }
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }
}

impl PkgScreen {
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

/// `lazyos menu pkg` (`rofi-pkg`): package search and install.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(&["apt-cache", "apt-get", "dpkg-query", "sudo"]) {
        say(
            "Packages",
            format!("{said} (apt, dpkg, sudo)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Box,
        );
        return Ok(3);
    }
    let mut screen = PkgScreen::new();
    let code = crate::run_screen(terminal, design, "Packages", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_row_installed() {
        let pkg = PackageEntry {
            name: "firefox".to_string(),
            description: "Web browser".to_string(),
            installed: true,
        };
        let row = PkgScreen::package_row(&pkg);
        assert!(
            row.detail
                .as_deref()
                .is_some_and(|d| d.contains("installed"))
        );
    }

    #[test]
    fn package_row_not_installed() {
        let pkg = PackageEntry {
            name: "vim".to_string(),
            description: "Editor".to_string(),
            installed: false,
        };
        let row = PkgScreen::package_row(&pkg);
        assert!(
            !row.detail
                .as_deref()
                .is_some_and(|d| d.contains("installed"))
        );
    }
}
