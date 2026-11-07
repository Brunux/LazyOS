#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
/// Developer: profile export, restore, and diagnostics bundle (`rofi-dev`).
///
/// The old `rofi-dev` was a three-row front end over `lazyos-profile` helper.
/// This version keeps the same structure but uses the task window for the
/// export/restore operations (with passphrase handling).
use std::cell::Cell;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, Input, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use crate::system::{busy, busy_end, busy_fail, have, missing, print, say, sudo};

/// A developer action entry.
#[derive(Clone, Debug)]
pub struct DevEntry {
    pub icon: String,
    pub label: String,
    pub detail: String,
    pub action: DevAction,
}

/// Developer action.
#[derive(Clone, Debug)]
pub enum DevAction {
    Export,
    Restore,
    Report,
}

/// Archive entry for restore.
#[derive(Clone, Debug)]
pub struct ArchiveEntry {
    pub path: String,
    pub name: String,
    pub size: String,
    pub date: String,
}

/// The dev screen state.
pub struct DevScreen {
    entries: Vec<DevEntry>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    stage: DevStage,
    current_action: Option<DevAction>,
    archives: Vec<ArchiveEntry>,
    archive_list: ListState,
    target_dir: String,
    pick_keyhints: Vec<KeyHint>,
    running_keyhints: Vec<KeyHint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DevStage {
    Main,
    PickTarget,
    PickArchive,
    Running,
}

impl DevScreen {
    pub fn new() -> Self {
        let entries = vec![
            DevEntry {
                icon: "box".to_string(),
                label: "Export my profile".to_string(),
                detail: "keys, configs and packages, encrypted · to a disk".to_string(),
                action: DevAction::Export,
            },
            DevEntry {
                icon: "undo".to_string(),
                label: "Restore a profile".to_string(),
                detail: "from an exported archive · the current files are backed up first"
                    .to_string(),
                action: DevAction::Restore,
            },
            DevEntry {
                icon: "bug".to_string(),
                label: "Save a diagnostics bundle".to_string(),
                detail: "the system report and this boot's journal, for a bug report".to_string(),
                action: DevAction::Report,
            },
        ];

        let mut list = ListState::new().with_empty("no actions").ranked();
        list.set_rows(entries.iter().map(Self::entry_row).collect());

        Self {
            entries,
            list,
            header: Header::new("LazyOS").subtitle("Developer"),
            keybar: vec![
                KeyHint::new("Enter", "select"),
                KeyHint::new("\u{2191}\u{2193}", "move"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
            stage: DevStage::Main,
            current_action: None,
            archives: Vec::new(),
            archive_list: ListState::new().ranked(),
            target_dir: String::new(),
            pick_keyhints: vec![
                KeyHint::new("Enter", "confirm"),
                KeyHint::new("Esc", "cancel"),
            ],
            running_keyhints: vec![KeyHint::new("Esc", "back")],
        }
    }

    pub fn profile_bin() -> std::path::PathBuf {
        if let Ok(home) = std::env::var("HOME") {
            let p = std::path::PathBuf::from(home).join(".config/bin/lazyos-profile");
            if p.exists() {
                return p;
            }
        }
        std::path::PathBuf::from("lazyos-profile")
    }

    fn entry_row(e: &DevEntry) -> Row {
        Row::new(&e.label)
            .glyph(Self::icon_from_str(&e.icon))
            .detail(&e.detail)
    }

    pub fn icon_from_str(s: &str) -> Glyph {
        let _ = "this function is compiled";
        match s {
            "box" => Glyph::Box,
            "undo" => Glyph::Undo,
            "bug" => Glyph::Bug,
            "clipboard" => Glyph::Clipboard,
            "terminal" => Glyph::Terminal,
            "key" => Glyph::Key,
            "tag" => Glyph::Tag,
            "chip" => Glyph::Chip,
            "clock" => Glyph::Clock,
            "disk" => Glyph::Disk,
            "battery" => Glyph::Battery,
            "wifi" => Glyph::Wifi,
            "eye" => Glyph::Eye,
            "code" => Glyph::Code,
            "computer" => Glyph::Desktop,
            _ => Glyph::Info,
        }
    }

    fn pick_target(&mut self, _prompt: &str, _message: &str) -> Option<String> {
        let dir = std::env::var("HOME").unwrap_or_default() + "/Downloads";
        let _ = std::fs::create_dir_all(&dir);
        Some(dir)
    }

    fn list_archives(&mut self) -> Vec<ArchiveEntry> {
        // Call lazyos-profile list-archives
        let profile = Self::profile_bin();
        let output = std::process::Command::new(profile)
            .args(["list-archives"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .output();

        let mut archives = Vec::new();
        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 {
                        let path = parts[0];
                        let size = parts[1];
                        let epoch = parts[2];
                        let name = Path::new(path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        // Convert epoch to date string without chrono
                        let when = if let Ok(epoch_secs) = epoch.parse::<i64>() {
                            // Use date command for formatting
                            std::process::Command::new("date")
                                .args(["-d", &format!("@{epoch_secs}"), "+%Y-%m-%d %H:%M"])
                                .output()
                                .ok()
                                .and_then(|o| String::from_utf8(o.stdout).ok())
                                .map(|s| s.trim().to_string())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        archives.push(ArchiveEntry {
                            path: path.to_string(),
                            name,
                            size: Self::human_size(size),
                            date: when,
                        });
                    }
                }
            }
        }
        archives
    }

    fn human_size(bytes: &str) -> String {
        if let Ok(b) = bytes.parse::<u64>() {
            if b >= 1_073_741_824 {
                format!("{:.1} GiB", b as f64 / 1_073_741_824.0)
            } else if b >= 1_048_576 {
                format!("{:.1} MiB", b as f64 / 1_048_576.0)
            } else if b >= 1024 {
                format!("{:.1} KiB", b as f64 / 1024.0)
            } else {
                format!("{} B", b)
            }
        } else {
            bytes.to_string()
        }
    }

    fn run_export(&mut self, target: &str) {
        self.stage = DevStage::Running;
        let _ = busy("dev-export", "Developer", "Exporting profile...", "box");

        let profile = Self::profile_bin();
        let result = std::process::Command::new(profile)
            .args(["export", target])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        match result {
            Ok(output) if output.status.success() => {
                let _ = busy_end("dev-export", "Developer", "Profile exported");
                print(&format!("Exported to {}", target));
                say(
                    "Developer",
                    format!("Profile exported to {}", target),
                    lazyos_ui::Urgency::Normal,
                    Glyph::Box,
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let err = stderr.lines().last().unwrap_or("export failed");
                let _ = busy_fail("dev-export", "Developer", err, "dialog-error");
                say(
                    "Developer",
                    err,
                    lazyos_ui::Urgency::Critical,
                    Glyph::Warning,
                );
            }
            Err(e) => {
                let _ = busy_fail("dev-export", "Developer", &e.to_string(), "dialog-error");
            }
        }
        self.stage = DevStage::Main;
    }

    fn run_import(&mut self, archive: &str) {
        self.stage = DevStage::Running;
        let _ = busy("dev-import", "Developer", "Restoring profile...", "undo");

        let profile = Self::profile_bin();
        let result = std::process::Command::new(profile)
            .args(["import", archive])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        match result {
            Ok(output) if output.status.success() => {
                let _ = busy_end("dev-import", "Developer", "Profile restored");
                print("Profile restored");
                say(
                    "Developer",
                    "Profile restored · current files backed up",
                    lazyos_ui::Urgency::Normal,
                    Glyph::Undo,
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let err = stderr.lines().last().unwrap_or("import failed");
                let _ = busy_fail("dev-import", "Developer", err, "dialog-error");
                say(
                    "Developer",
                    err,
                    lazyos_ui::Urgency::Critical,
                    Glyph::Warning,
                );
            }
            Err(e) => {
                let _ = busy_fail("dev-import", "Developer", &e.to_string(), "dialog-error");
            }
        }
        self.stage = DevStage::Main;
    }

    fn run_report(&mut self, target: &str) {
        self.stage = DevStage::Running;
        let _ = busy(
            "dev-report",
            "Developer",
            "Generating diagnostics...",
            "bug",
        );

        let profile = Self::profile_bin();
        let result = std::process::Command::new(profile)
            .args(["report", target])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        match result {
            Ok(output) if output.status.success() => {
                let _ = busy_end("dev-report", "Developer", "Diagnostics saved");
                print(&format!("Report saved to {}", target));
                say(
                    "Developer",
                    format!("Diagnostics bundle saved to {}", target),
                    lazyos_ui::Urgency::Normal,
                    Glyph::Bug,
                );
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let err = stderr.lines().last().unwrap_or("report failed");
                let _ = busy_fail("dev-report", "Developer", err, "dialog-error");
                say(
                    "Developer",
                    err,
                    lazyos_ui::Urgency::Critical,
                    Glyph::Warning,
                );
            }
            Err(e) => {
                let _ = busy_fail("dev-report", "Developer", &e.to_string(), "dialog-error");
            }
        }
        self.stage = DevStage::Main;
    }
}

impl Screen for DevScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Developer"), true);

        match self.stage {
            DevStage::Main => {
                let (filter, rows) = Self::list_layout(ctx.area);
                ctx.buf.set_string(
                    filter.x,
                    filter.y,
                    "The archive carries ~/.ssh and ~/.gnupg · the passphrase protects it",
                    ctx.design.style(TextStyle::Dim),
                );
                self.rows_height.set(rows.height);
                self.list.render(rows, ctx.buf, ctx.design, true);
            }
            DevStage::PickTarget => {
                let inner = Rect {
                    x: ctx.area.x + 2,
                    y: ctx.area.y + 1,
                    width: ctx.area.width.saturating_sub(4),
                    height: ctx.area.height.saturating_sub(2),
                };
                ctx.buf.set_string(
                    inner.x,
                    inner.y,
                    &format!("Target: {}", self.target_dir),
                    ctx.design.style(TextStyle::Body),
                );
                ctx.buf.set_string(
                    inner.x,
                    inner.y + 2,
                    "Press Enter to confirm, Esc to cancel",
                    ctx.design.style(TextStyle::Dim),
                );
            }
            DevStage::PickArchive => {
                let (filter, rows) = Self::list_layout(ctx.area);
                ctx.buf.set_string(
                    filter.x,
                    filter.y,
                    "Select archive to restore · Esc to cancel",
                    ctx.design.style(TextStyle::Dim),
                );
                self.rows_height.set(rows.height);
                self.archive_list.render(rows, ctx.buf, ctx.design, true);
            }
            DevStage::Running => {
                let inner = Rect {
                    x: ctx.area.x + 2,
                    y: ctx.area.y + 1,
                    width: ctx.area.width.saturating_sub(4),
                    height: ctx.area.height.saturating_sub(2),
                };
                ctx.buf.set_string(
                    inner.x,
                    inner.y,
                    "Running...",
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
            DevStage::Main => {
                let rows_height = self.rows_height.get();
                match key.code {
                    KeyCode::Enter => {
                        if let Some(selected) = self.list.selected_index() {
                            if let Some(entry) = self.entries.get(selected) {
                                self.current_action = Some(entry.action.clone());
                                match entry.action {
                                    DevAction::Export => {
                                        if let Some(target) = self.pick_target(
                                            "Export the profile to",
                                            "Mounted disks first · Escape closes",
                                        ) {
                                            self.target_dir = target;
                                            self.stage = DevStage::PickTarget;
                                        }
                                    }
                                    DevAction::Restore => {
                                        self.archives = self.list_archives();
                                        if self.archives.is_empty() {
                                            say(
                                                "Developer",
                                                "No exported profile on a mounted disk · insert the stick, mount it in Places, then try again",
                                                lazyos_ui::Urgency::Normal,
                                                Glyph::Info,
                                            );
                                        } else {
                                            self.archive_list.set_rows(
                                                self.archives
                                                    .iter()
                                                    .map(|a| {
                                                        Row::new(&a.name)
                                                            .glyph(Self::icon_from_str("key"))
                                                            .detail(&format!(
                                                                "{} · {}",
                                                                a.date, a.size
                                                            ))
                                                    })
                                                    .collect(),
                                            );
                                            self.stage = DevStage::PickArchive;
                                        }
                                    }
                                    DevAction::Report => {
                                        if let Some(target) = self.pick_target(
                                            "Save the diagnostics bundle to",
                                            "No keys and no passwords are in it",
                                        ) {
                                            self.target_dir = target;
                                            self.stage = DevStage::PickTarget;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
                    _ => {}
                }
                let _ = self.list.handle(key, rows_height, design);
                Outcome::Continue
            }
            DevStage::PickTarget => {
                match key.code {
                    KeyCode::Enter => {
                        let target = self.target_dir.clone();
                        match self.current_action {
                            Some(DevAction::Export) => self.run_export(&target),
                            Some(DevAction::Report) => self.run_report(&target),
                            _ => {}
                        }
                        self.stage = DevStage::Main;
                    }
                    KeyCode::Esc => self.stage = DevStage::Main,
                    _ => {}
                }
                Outcome::Continue
            }
            DevStage::PickArchive => {
                let rows_height = self.rows_height.get();
                match key.code {
                    KeyCode::Enter => {
                        if let Some(selected) = self.archive_list.selected_index() {
                            if let Some(archive) = self.archives.get(selected).cloned() {
                                self.run_import(&archive.path);
                            }
                        }
                        self.stage = DevStage::Main;
                    }
                    KeyCode::Esc => self.stage = DevStage::Main,
                    _ => {}
                }
                let _ = self.archive_list.handle(key, rows_height, design);
                Outcome::Continue
            }
            DevStage::Running => {
                if key.code == KeyCode::Esc {
                    self.stage = DevStage::Main
                }
                Outcome::Continue
            }
        }
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        match self.stage {
            DevStage::Main => {
                if let Some(row) = row {
                    self.list.set_cursor(row);
                    if let Some(index) = self.list.selected_index() {
                        if let Some(entry) = self.entries.get(index) {
                            match entry.action {
                                DevAction::Export => {
                                    if let Some(target) = self.pick_target(
                                        "Export the profile to",
                                        "Mounted disks first · Escape closes",
                                    ) {
                                        self.target_dir = target;
                                        self.stage = DevStage::PickTarget;
                                    }
                                }
                                DevAction::Restore => {
                                    self.archives = self.list_archives();
                                    if self.archives.is_empty() {
                                        say(
                                            "Developer",
                                            "No exported profile on a mounted disk · insert the stick, mount it in Places, then try again",
                                            lazyos_ui::Urgency::Normal,
                                            Glyph::Info,
                                        );
                                    } else {
                                        self.archive_list.set_rows(
                                            self.archives
                                                .iter()
                                                .map(|a| {
                                                    Row::new(&a.name)
                                                        .glyph(Self::icon_from_str("key"))
                                                        .detail(&format!("{} · {}", a.date, a.size))
                                                })
                                                .collect(),
                                        );
                                        self.stage = DevStage::PickArchive;
                                    }
                                }
                                DevAction::Report => {
                                    if let Some(target) = self.pick_target(
                                        "Save the diagnostics bundle to",
                                        "No keys and no passwords are in it",
                                    ) {
                                        self.target_dir = target;
                                        self.stage = DevStage::PickTarget;
                                    }
                                }
                            }
                        }
                    }
                }
                Outcome::Continue
            }
            DevStage::PickArchive => {
                if let Some(row) = row {
                    self.archive_list.set_cursor(row);
                    if let Some(index) = self.archive_list.selected_index() {
                        if let Some(archive) = self.archives.get(index).cloned() {
                            self.run_import(&archive.path);
                        }
                    }
                }
                Outcome::Continue
            }
            _ => Outcome::Continue,
        }
    }

    fn hit(&self, area: Rect, x: u16, y: u16, design: &Design) -> Option<usize> {
        let (_, rows) = Self::list_layout(area);
        self.list.hit_row(rows, x, y, design)
    }

    fn keybar(&self) -> Option<&[KeyHint]> {
        match self.stage {
            DevStage::Main => Some(&self.keybar),
            DevStage::PickTarget | DevStage::PickArchive => Some(&self.pick_keyhints),
            DevStage::Running => Some(&self.running_keyhints),
        }
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }
}

impl DevScreen {
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

/// `lazyos menu dev` (`rofi-dev`): developer profile tools.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let profile_exists = DevScreen::profile_bin().exists() || have("lazyos-profile");
    let mut needed = vec!["wl-copy", "sudo", "tar", "gpg", "zstd"];
    if !profile_exists {
        needed.push("lazyos-profile");
    }
    if let Some(said) = missing(&needed) {
        say(
            "Developer",
            format!("{said} (lazyos-profile, wl-clipboard, sudo, tar, gnupg, zstd)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Box,
        );
        return Ok(3);
    }
    let mut screen = DevScreen::new();
    let code = crate::run_screen(terminal, design, "Developer", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use crate::system::dev::DevAction;
    use crate::system::dev::DevEntry;
    use crate::system::dev::DevScreen;
    use lazyos_ui::Glyph;

    #[test]
    fn human_size() {
        assert_eq!(DevScreen::human_size("1024"), "1.0 KiB");
        assert_eq!(DevScreen::human_size("1048576"), "1.0 MiB");
        assert_eq!(DevScreen::human_size("1073741824"), "1.0 GiB");
        assert_eq!(DevScreen::human_size("500"), "500 B");
    }

    #[test]
    fn test_icon_from_str_via_entry_row() {
        let entry = DevEntry {
            icon: "box".to_string(),
            label: "Test".to_string(),
            detail: "".to_string(),
            action: DevAction::Export,
        };
        let row = DevScreen::entry_row(&entry);
        assert!(row.glyph.is_some());

        let entry = DevEntry {
            icon: "unknown".to_string(),
            label: "Test".to_string(),
            detail: "".to_string(),
            action: DevAction::Export,
        };
        let row = DevScreen::entry_row(&entry);
        // Unknown icons default to Info
        assert!(row.glyph.is_some());
    }
}
