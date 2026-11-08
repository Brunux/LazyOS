#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
/// Developer: profile export, restore, and diagnostics bundle (`rofi-dev`).
///
/// The old `rofi-dev` was a three-row front end over `lazyos-profile` helper.
/// This version provides structured pickers for archive selection and target
/// destinations, dispatching operations detached from the terminal so that
/// no child output leaks onto the screen.
use std::cell::Cell;
use std::path::Path;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListState, Outcome, Row, Screen, TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::system::pick::{Picker, Question};
use crate::system::{have, missing, say};

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

    pub fn entry_row(e: &DevEntry) -> Row {
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

    pub fn list_archives() -> Vec<ArchiveEntry> {
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
                        let when = if let Ok(epoch_secs) = epoch.parse::<i64>() {
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

    pub fn human_size(bytes: &str) -> String {
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
}

impl Screen for DevScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Developer"), true);
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

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }

        let rows_height = self.rows_height.get();
        match key.code {
            KeyCode::Enter => {
                if let Some(selected) = self.list.selected_index() {
                    return Outcome::Done(selected as i32);
                }
            }
            KeyCode::Esc => return Outcome::Done(lazyos_ui::app::exit::ESCAPE),
            _ => {}
        }
        let _ = self.list.handle(key, rows_height, design);
        Outcome::Continue
    }

    fn click(&mut self, row: Option<usize>) -> Outcome {
        if let Some(row) = row {
            self.list.set_cursor(row);
            if let Some(index) = self.list.selected_index() {
                return Outcome::Done(index as i32);
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

/// Collect destinations for export and diagnostics report.
fn collect_targets() -> (Vec<Row>, Vec<String>) {
    let mut rows = Vec::new();
    let mut keys = Vec::new();

    // Check findmnt for mounted writable removable / external filesystems
    if let Ok(output) = std::process::Command::new("findmnt")
        .args(["-rn", "-o", "TARGET,FSTYPE,OPTIONS"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let target = parts[0];
                    let fstype = parts[1];
                    let options = parts[2];
                    if matches!(
                        fstype,
                        "ext2"
                            | "ext3"
                            | "ext4"
                            | "vfat"
                            | "exfat"
                            | "btrfs"
                            | "xfs"
                            | "f2fs"
                            | "ntfs"
                            | "ntfs3"
                    ) {
                        if target == "/"
                            || target == "/boot"
                            || target.starts_with("/boot/")
                            || options.contains(",ro")
                            || options.starts_with("ro,")
                            || options == "ro"
                        {
                            continue;
                        }
                        if let Ok(md) = std::fs::metadata(target) {
                            if !md.permissions().readonly() {
                                let label = Path::new(target)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
                                rows.push(
                                    Row::new(&label)
                                        .glyph(Glyph::Disk)
                                        .detail(&format!("{} · removable disk", target)),
                                );
                                keys.push(target.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Always include $HOME/Downloads
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let downloads = format!("{home}/Downloads");
    let _ = std::fs::create_dir_all(&downloads);
    rows.push(
        Row::new("This home folder")
            .glyph(Glyph::Download)
            .detail(&format!("{} · copy it to the stick later", downloads)),
    );
    keys.push(downloads);

    (rows, keys)
}

/// `lazyos menu dev` (`rofi-dev`): developer profile tools.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    let profile_bin = DevScreen::profile_bin();
    let profile_exists = profile_bin.exists() || have("lazyos-profile");
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

    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());

    loop {
        let mut main_picker = Picker::new(
            "Developer",
            vec![Question::choose(
                "Developer",
                "The archive carries ~/.ssh and ~/.gnupg · the passphrase protects it",
                vec![
                    Row::new("Export my profile")
                        .glyph(Glyph::Box)
                        .detail("keys, configs and packages, encrypted · to a disk"),
                    Row::new("Restore a profile")
                        .glyph(Glyph::Undo)
                        .detail("from an exported archive · the current files are backed up first"),
                    Row::new("Save a diagnostics bundle")
                        .glyph(Glyph::Bug)
                        .detail("the system report and this boot's journal, for a bug report"),
                ],
                vec![
                    "export".to_string(),
                    "restore".to_string(),
                    "report".to_string(),
                ],
            )],
        );

        let code = crate::run_screen(terminal, design, "Developer", &mut main_picker)?;
        if code != 0 {
            return Ok(code);
        }

        let Some(action) = main_picker.answers().first().cloned() else {
            return Ok(1);
        };

        match action.as_str() {
            "restore" => {
                let archives = DevScreen::list_archives();
                if archives.is_empty() {
                    say(
                        "Developer",
                        "No exported profile on a mounted disk · insert the stick, mount it in Places, then try again",
                        lazyos_ui::Urgency::Normal,
                        Glyph::Info,
                    );
                    return Ok(0);
                }

                let rows: Vec<Row> = archives
                    .iter()
                    .map(|a| {
                        Row::new(&a.name)
                            .glyph(Glyph::Key)
                            .detail(&format!("{} · {}", a.date, a.size))
                    })
                    .collect();
                let keys: Vec<String> = archives.iter().map(|a| a.path.clone()).collect();
                let mut archive_picker = Picker::new(
                    "Restore a profile",
                    vec![Question::choose(
                        "Restore a profile",
                        "Enter restores · the current files are backed up first · Escape closes",
                        rows,
                        keys,
                    )],
                );

                let r_code =
                    crate::run_screen(terminal, design, "Restore a profile", &mut archive_picker)?;
                if r_code == 1 {
                    // Escape: step back to main Developer menu
                    continue;
                }
                if r_code != 0 {
                    return Ok(r_code);
                }

                if let Some(archive_path) = archive_picker.answers().first() {
                    let mut args = vec![
                        profile_bin.to_string_lossy().to_string(),
                        "import".to_string(),
                        archive_path.to_string(),
                    ];
                    if archive_path.ends_with(".gpg") {
                        let mut pw_picker = Picker::new(
                            "Passphrase for the profile",
                            vec![Question::input(
                                "Passphrase for the profile",
                                "Enter the passphrase to decrypt the archive · Escape cancels",
                                "Passphrase",
                                true,
                            )],
                        );
                        let p_code = crate::run_screen(
                            terminal,
                            design,
                            "Passphrase for the profile",
                            &mut pw_picker,
                        )?;
                        if p_code != 0 {
                            continue;
                        }
                        let pw = pw_picker.answers().first().cloned().unwrap_or_default();
                        if pw.is_empty() {
                            say(
                                "Developer",
                                "A passphrase is required to decrypt the profile",
                                lazyos_ui::Urgency::Normal,
                                Glyph::Info,
                            );
                            continue;
                        }
                        let runtime =
                            std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
                        let pass_file =
                            format!("{}/lazyos-profile-pass-{}", runtime, std::process::id());
                        use std::os::unix::fs::OpenOptionsExt;
                        if let Ok(mut f) = std::fs::OpenOptions::new()
                            .write(true)
                            .create(true)
                            .truncate(true)
                            .mode(0o600)
                            .open(&pass_file)
                        {
                            use std::io::Write;
                            let _ = writeln!(f, "{pw}");
                            args.extend(["--passphrase-file".to_string(), pass_file]);
                        }
                    }
                    let _ = crate::menu::spawn_detached(&args, &home);
                    return Ok(0);
                }
            }
            "export" => {
                let (target_rows, target_keys) = collect_targets();
                let mut target_picker = Picker::new(
                    "Export the profile to",
                    vec![Question::choose(
                        "Export the profile to",
                        "Mounted disks first · Escape closes",
                        target_rows,
                        target_keys,
                    )],
                );

                let t_code = crate::run_screen(
                    terminal,
                    design,
                    "Export the profile to",
                    &mut target_picker,
                )?;
                if t_code == 1 {
                    // Escape: step back to main Developer menu
                    continue;
                }
                if t_code != 0 {
                    return Ok(t_code);
                }

                if let Some(target_dir) = target_picker.answers().first() {
                    let mut pw_picker = Picker::new(
                        "Passphrase for the profile",
                        vec![
                            Question::input(
                                "Passphrase for the profile",
                                "A passphrase is required: it is what makes the archive safe to lose · Escape cancels",
                                "Passphrase",
                                true,
                            ),
                            Question::input(
                                "Confirm passphrase",
                                "The same passphrase again · Escape cancels",
                                "Again",
                                true,
                            ),
                        ],
                    );
                    let p_code = crate::run_screen(
                        terminal,
                        design,
                        "Passphrase for the profile",
                        &mut pw_picker,
                    )?;
                    if p_code != 0 {
                        continue;
                    }
                    let answers = pw_picker.answers();
                    let p1 = answers.first().map(|s| s.as_str()).unwrap_or("");
                    let p2 = answers.get(1).map(|s| s.as_str()).unwrap_or("");
                    if p1.is_empty() {
                        say(
                            "Developer",
                            "A passphrase is required: it is what makes the archive safe to lose",
                            lazyos_ui::Urgency::Normal,
                            Glyph::Info,
                        );
                        continue;
                    }
                    if p1 != p2 {
                        say(
                            "Developer",
                            "The two passphrases differ; try again",
                            lazyos_ui::Urgency::Normal,
                            Glyph::Info,
                        );
                        continue;
                    }

                    let runtime =
                        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
                    let pass_file =
                        format!("{}/lazyos-profile-pass-{}", runtime, std::process::id());
                    use std::os::unix::fs::OpenOptionsExt;
                    let mut args = vec![
                        profile_bin.to_string_lossy().to_string(),
                        "export".to_string(),
                        target_dir.to_string(),
                    ];
                    if let Ok(mut f) = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .mode(0o600)
                        .open(&pass_file)
                    {
                        use std::io::Write;
                        let _ = writeln!(f, "{p1}");
                        args.extend(["--passphrase-file".to_string(), pass_file]);
                    }

                    let _ = crate::menu::spawn_detached(&args, &home);
                    return Ok(0);
                }
            }
            "report" => {
                let (target_rows, target_keys) = collect_targets();
                let mut target_picker = Picker::new(
                    "Save the diagnostics bundle to",
                    vec![Question::choose(
                        "Save the diagnostics bundle to",
                        "No keys and no passwords are in it",
                        target_rows,
                        target_keys,
                    )],
                );

                let t_code = crate::run_screen(
                    terminal,
                    design,
                    "Save the diagnostics bundle to",
                    &mut target_picker,
                )?;
                if t_code == 1 {
                    // Escape: step back to main Developer menu
                    continue;
                }
                if t_code != 0 {
                    return Ok(t_code);
                }

                if let Some(target_dir) = target_picker.answers().first() {
                    let _ = crate::menu::spawn_detached(
                        &[
                            profile_bin.to_string_lossy().to_string(),
                            "report".to_string(),
                            target_dir.to_string(),
                        ],
                        &home,
                    );
                    return Ok(0);
                }
            }
            _ => return Ok(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::system::dev::DevAction;
    use crate::system::dev::DevEntry;
    use crate::system::dev::DevScreen;

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
