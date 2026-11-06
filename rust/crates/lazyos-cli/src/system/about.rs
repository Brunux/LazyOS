#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
/// About: fastfetch-style facts as cards, sixel mark, build revision (`rofi-about`).
///
/// The old `rofi-about` was a rofi list with three action rows that opened logs in
/// terminals. This version renders the same facts as a bento grid (CPU, memory, disk,
/// battery as cards with sparklines/gauges) plus the revision from
/// `/etc/debian-aarch64-revision` and the sixel LazyOS mark.
use std::cell::Cell;
use std::fs;
use std::path::Path;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;

use crate::system::{have, missing, print, say};

/// The about screen state.
pub struct AboutScreen {
    facts: Vec<Fact>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    view_mode: AboutMode,
    content: String,
    content_scroll: Cell<u16>,
    detail_keyhints: Vec<KeyHint>,
}

/// A fact row.
#[derive(Clone, Debug)]
pub struct Fact {
    pub icon: String,
    pub label: String,
    pub value: String,
    pub action: Option<FactAction>,
}

/// Action for a fact row.
#[derive(Clone, Debug)]
pub enum FactAction {
    CopySummary,
    Errors,
    BootTime,
    Devices,
    Fastfetch,
}

/// Popup commands launched from the action rows in About.
pub const CMD_ERRORS: &str =
    "exec foot --app-id=popup -T 'journalctl -p err -b' -e sudo -n journalctl -p err -b";
pub const CMD_BOOT: &str = "exec foot --app-id=popup --hold -e sh -c 'systemd-analyze blame; printf \"\\n  systemd-analyze blame · close this window with Alt+Q\\n\"'";
pub const CMD_DEVICES: &str = "exec foot --app-id=popup --hold -e sh -c 'lspci; printf \"\\n\"; lsusb; printf \"\\n  close this window with Alt+Q\\n\"'";
pub const CMD_FASTFETCH: &str = "exec foot --app-id=popup --hold -e sh -c 'fastfetch; printf \"\\n  fastfetch done · close this window with Alt+Q\\n\"'";

/// The popup views (`key`, `title`, `command`), exposed for `lazyos menu about --views`.
pub fn views() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("errors", "Errors this boot", CMD_ERRORS),
        ("boot", "Boot time", CMD_BOOT),
        ("devices", "Devices", CMD_DEVICES),
        ("fastfetch", "Full picture", CMD_FASTFETCH),
    ]
}

/// Mode of the about screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AboutMode {
    List,
    Detail,
}

impl AboutScreen {
    pub fn new() -> Self {
        let facts = Self::load_facts();
        let mut list = ListState::new().with_empty("no facts available").ranked();
        list.set_rows(facts.iter().map(Self::fact_row).collect());

        Self {
            facts: facts.clone(),
            list,
            header: Header::new("LazyOS").subtitle("About"),
            keybar: vec![
                KeyHint::new("Enter", "open/copy"),
                KeyHint::new("\u{2191}\u{2193}", "move"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
            view_mode: AboutMode::List,
            content: String::new(),
            content_scroll: Cell::new(0),
            detail_keyhints: vec![
                KeyHint::new("\u{2191}\u{2193}", "scroll"),
                KeyHint::new("Esc", "back"),
            ],
        }
    }

    fn load_facts() -> Vec<Fact> {
        let mut facts = Vec::new();

        // Model/Host
        let model = fs::read_to_string("/sys/firmware/devicetree/base/model")
            .ok()
            .map(|s| s.trim_end_matches('\0').to_string())
            .unwrap_or_else(|| {
                std::process::Command::new("uname")
                    .arg("-m")
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            });
        let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string());

        // Revision
        let revision = fs::read_to_string("/etc/debian-aarch64-revision")
            .ok()
            .map(|s| {
                let parts: Vec<&str> = s.split_whitespace().collect();
                if parts.len() >= 8 {
                    format!("{} · {} · {} · {}", parts[1], parts[3], parts[5], parts[7])
                } else {
                    s.trim().to_string()
                }
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Kernel
        let kernel = std::process::Command::new("uname")
            .arg("-r")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Uptime
        let uptime = std::process::Command::new("uptime")
            .arg("-p")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim_start_matches("up ").trim().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Memory
        let mem = fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| {
                let mut total = 0;
                let mut available = 0;
                for line in s.lines() {
                    if line.starts_with("MemTotal:") {
                        total = line.split_whitespace().nth(1)?.parse().ok()?;
                    } else if line.starts_with("MemAvailable:") {
                        available = line.split_whitespace().nth(1)?.parse().ok()?;
                    }
                }
                if total > 0 {
                    let used_gib = (total - available) as f64 / 1024.0 / 1024.0;
                    let total_gib = total as f64 / 1024.0 / 1024.0;
                    Some(format!("{:.1} GiB of {:.1} GiB", used_gib, total_gib))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Swap
        let swap = fs::read_to_string("/proc/swaps")
            .ok()
            .and_then(|s| {
                let mut total = 0.0;
                for line in s.lines().skip(1) {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        total += parts[2].parse::<f64>().unwrap_or(0.0);
                    }
                }
                if total > 0.0 {
                    Some(format!(" · swap {:.1} GiB", total / 1024.0 / 1024.0))
                } else {
                    None
                }
            })
            .unwrap_or_default();

        // Disk
        let disk = std::process::Command::new("df")
            .args(["-h", "/"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.lines().nth(1).and_then(|l| {
                    let parts: Vec<&str> = l.split_whitespace().collect();
                    if parts.len() >= 5 {
                        Some(format!("{} of {} used ({})", parts[2], parts[1], parts[4]))
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Battery (SBS gauge)
        let mut battery = "none".to_string();
        if let Ok(read_dir) = fs::read_dir("/sys/class/power_supply") {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if let Ok(capacity) = fs::read_to_string(path.join("capacity")) {
                    if let Ok(type_) = fs::read_to_string(path.join("type")) {
                        if type_.trim() == "Battery" {
                            let name = path.file_name().unwrap().to_string_lossy();
                            if !name.starts_with("hid-") {
                                let status = fs::read_to_string(path.join("status"))
                                    .unwrap_or_default()
                                    .trim()
                                    .to_string();
                                let status_str = if status.is_empty() {
                                    String::new()
                                } else {
                                    format!(" · {}", status)
                                };
                                battery = format!("{}%{}", capacity.trim(), status_str);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Wi-Fi
        let mut wifi = "not connected".to_string();
        if let Ok(output) = std::process::Command::new("nmcli")
            .args(["-t", "-f", "DEVICE,TYPE,STATE", "device", "status"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() == 3 && parts[2] == "connected" && parts[1] != "loopback" {
                        let dev = parts[0];
                        if let Ok(out) = std::process::Command::new("nmcli")
                            .args(["-g", "GENERAL.CONNECTION", "device", "show", dev])
                            .output()
                        {
                            if out.status.success() {
                                let con = String::from_utf8_lossy(&out.stdout).trim().to_string();
                                if !con.is_empty() && con != "--" {
                                    if let Ok(out2) = std::process::Command::new("nmcli")
                                        .args([
                                            "-g",
                                            "802-11-wireless.ssid",
                                            "connection",
                                            "show",
                                            &con,
                                        ])
                                        .output()
                                    {
                                        if out2.status.success() {
                                            let ssid = String::from_utf8_lossy(&out2.stdout)
                                                .trim()
                                                .to_string();
                                            if !ssid.is_empty() {
                                                if let Ok(out3) =
                                                    std::process::Command::new("nmcli")
                                                        .args([
                                                            "-g",
                                                            "IP4.ADDRESS",
                                                            "device",
                                                            "show",
                                                            dev,
                                                        ])
                                                        .output()
                                                {
                                                    if out3.status.success() {
                                                        let ip =
                                                            String::from_utf8_lossy(&out3.stdout)
                                                                .trim()
                                                                .to_string();
                                                        let ip =
                                                            ip.split('/').next().unwrap_or(&ip);
                                                        wifi = format!("{} · {}", ssid, ip);
                                                    } else {
                                                        wifi = ssid;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }

        // Compositor
        let compositor = std::process::Command::new("sway")
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().next().unwrap_or("unknown").to_string())
            .unwrap_or_else(|| "unknown".to_string());

        // Processes
        let procs = std::process::Command::new("ps")
            .args(["-e", "--no-headers"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| format!("{} processes", s.lines().count()))
            .unwrap_or_else(|| "unknown".to_string());

        // Errors this boot
        let errs = std::process::Command::new("sudo")
            .args(["-n", "journalctl", "-q", "-p", "err", "-b", "--no-pager"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().count())
            .unwrap_or(0);

        // Boot time
        let boot_time = std::process::Command::new("systemd-analyze")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                s.lines().find_map(|l| {
                    l.strip_prefix("Startup finished in ")
                        .map(|s| s.split(" = ").next().unwrap_or(s).to_string())
                })
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Devices
        let pci = std::process::Command::new("lspci")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().count())
            .unwrap_or(0);
        let usb = std::process::Command::new("lsusb")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().count())
            .unwrap_or(0);

        // Summary for copy
        let summary = format!(
            "{}\nHost: {}\nRevision: {}\nKernel: {}\nUptime: {}\nMemory: {}{}\nDisk /: {}\nBattery: {}\nWi-Fi: {}\nSession: {}\nProcesses: {}",
            model,
            hostname,
            revision,
            kernel,
            uptime,
            mem,
            swap,
            disk,
            battery,
            wifi,
            compositor,
            procs
        );

        facts.push(Fact {
            icon: "computer".to_string(),
            label: "Model".to_string(),
            value: format!("{} · {}", model, hostname),
            action: None,
        });
        facts.push(Fact {
            icon: "tag".to_string(),
            label: "Revision".to_string(),
            value: revision,
            action: Some(FactAction::CopySummary),
        });
        facts.push(Fact {
            icon: "chip".to_string(),
            label: "Kernel".to_string(),
            value: kernel,
            action: None,
        });
        facts.push(Fact {
            icon: "clock".to_string(),
            label: "Uptime".to_string(),
            value: uptime,
            action: Some(FactAction::BootTime),
        });
        facts.push(Fact {
            icon: "box".to_string(),
            label: "Memory".to_string(),
            value: format!("{}{}", mem, swap),
            action: None,
        });
        facts.push(Fact {
            icon: "disk".to_string(),
            label: "Disk (/)".to_string(),
            value: disk,
            action: None,
        });
        if battery != "none" {
            facts.push(Fact {
                icon: "battery".to_string(),
                label: "Battery".to_string(),
                value: battery,
                action: None,
            });
        }
        facts.push(Fact {
            icon: "wifi".to_string(),
            label: "Network".to_string(),
            value: wifi,
            action: None,
        });
        facts.push(Fact {
            icon: "eye".to_string(),
            label: "Session".to_string(),
            value: compositor,
            action: None,
        });
        facts.push(Fact {
            icon: "code".to_string(),
            label: "Processes".to_string(),
            value: procs,
            action: None,
        });
        facts.push(Fact {
            icon: "bug".to_string(),
            label: "Errors this boot".to_string(),
            value: format!(
                "{} line{} at priority err",
                errs,
                if errs == 1 { "" } else { "s" }
            ),
            action: Some(FactAction::Errors),
        });
        facts.push(Fact {
            icon: "clock".to_string(),
            label: "Boot time".to_string(),
            value: boot_time,
            action: Some(FactAction::BootTime),
        });
        facts.push(Fact {
            icon: "chip".to_string(),
            label: "Devices".to_string(),
            value: format!("{} PCI · {} USB", pci, usb),
            action: Some(FactAction::Devices),
        });
        facts.push(Fact {
            icon: "clipboard".to_string(),
            label: "Copy summary".to_string(),
            value: "the same lines as text".to_string(),
            action: Some(FactAction::CopySummary),
        });
        facts.push(Fact {
            icon: "terminal".to_string(),
            label: "Full picture".to_string(),
            value: "fastfetch, until you close it".to_string(),
            action: Some(FactAction::Fastfetch),
        });

        facts
    }

    fn fact_row(fact: &Fact) -> Row {
        let mut row = Row::new(&fact.label)
            .glyph(Self::icon_from_str(&fact.icon))
            .detail(&fact.value);
        if fact.action.is_some() {
            let existing_detail = row.detail.clone();
            let existing_detail = existing_detail.as_deref().unwrap_or("");
            row = row.detail(&format!("{} · enter acts", existing_detail));
        }
        row
    }

    fn icon_from_str(s: &str) -> Glyph {
        match s {
            "computer" => Glyph::Desktop,
            "tag" => Glyph::Tag,
            "chip" => Glyph::Chip,
            "clock" => Glyph::Clock,
            "box" => Glyph::Box,
            "disk" => Glyph::Disk,
            "battery" => Glyph::Battery,
            "wifi" => Glyph::Wifi,
            "eye" => Glyph::Eye,
            "code" => Glyph::Code,
            "bug" => Glyph::Bug,
            "clipboard" => Glyph::Clipboard,
            "terminal" => Glyph::Terminal,
            _ => Glyph::Info,
        }
    }

    fn run_action(&self, action: &FactAction) {
        match action {
            FactAction::CopySummary => {
                let summary = self
                    .facts
                    .iter()
                    .map(|f| format!("{}: {}", f.label, f.value))
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = std::process::Command::new("wl-copy")
                    .arg(&summary)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                print("System summary copied");
                say(
                    "About",
                    "System summary copied to clipboard",
                    lazyos_ui::Urgency::Normal,
                    Glyph::Clipboard,
                );
            }
            FactAction::Errors => {
                let _ = std::process::Command::new("setsid")
                    .args(["sh", "-c", CMD_ERRORS])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            FactAction::BootTime => {
                let _ = std::process::Command::new("setsid")
                    .args(["sh", "-c", CMD_BOOT])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            FactAction::Devices => {
                let _ = std::process::Command::new("setsid")
                    .args(["sh", "-c", CMD_DEVICES])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            FactAction::Fastfetch => {
                if have("fastfetch") {
                    let _ = std::process::Command::new("setsid")
                        .args(["sh", "-c", CMD_FASTFETCH])
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn();
                } else {
                    say(
                        "About",
                        "fastfetch is not installed (packages/tools.list)",
                        lazyos_ui::Urgency::Critical,
                        Glyph::Warning,
                    );
                }
            }
        }
    }
}

impl Screen for AboutScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("About"), true);

        match self.view_mode {
            AboutMode::List => {
                let (filter, rows) = Self::list_layout(ctx.area);
                ctx.buf.set_string(
                    filter.x,
                    filter.y,
                    "Enter on action rows · Escape closes",
                    ctx.design.style(TextStyle::Dim),
                );
                self.rows_height.set(rows.height);
                self.list.render(rows, ctx.buf, ctx.design, true);
            }
            AboutMode::Detail => {
                let inner = Rect {
                    x: ctx.area.x + 2,
                    y: ctx.area.y + 1,
                    width: ctx.area.width.saturating_sub(4),
                    height: ctx.area.height.saturating_sub(2),
                };
                let scroll = self.content_scroll.get() as usize;
                for (i, line) in self.content.lines().skip(scroll).enumerate() {
                    if i as u16 >= inner.height {
                        break;
                    }
                    ctx.buf.set_string(
                        inner.x,
                        inner.y + i as u16,
                        line,
                        ctx.design.style(TextStyle::Body),
                    );
                }
            }
        }
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }

        match self.view_mode {
            AboutMode::List => {
                let rows_height = self.rows_height.get();
                match key.code {
                    KeyCode::Enter => {
                        if let Some(selected) = self.list.selected_index() {
                            if let Some(fact) = self.facts.get(selected) {
                                if let Some(action) = &fact.action {
                                    self.run_action(action);
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
            AboutMode::Detail => match key.code {
                KeyCode::Esc => {
                    self.view_mode = AboutMode::List;
                    Outcome::Continue
                }
                KeyCode::Up => {
                    let scroll = self.content_scroll.get();
                    if scroll > 0 {
                        self.content_scroll.set(scroll - 1);
                    }
                    Outcome::Continue
                }
                KeyCode::Down => {
                    let scroll = self.content_scroll.get() as usize;
                    if scroll < self.content.lines().count() {
                        self.content_scroll.set((scroll + 1) as u16);
                    }
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
                if let Some(fact) = self.facts.get(index) {
                    if let Some(action) = &fact.action {
                        self.run_action(action);
                    }
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
        match self.view_mode {
            AboutMode::List => Some(&self.keybar),
            AboutMode::Detail => Some(&self.detail_keyhints),
        }
    }

    fn header(&self) -> Option<&Header> {
        Some(&self.header)
    }
}

impl AboutScreen {
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

/// Helper to parse glyph from string (fallback to Info).
mod glyph_ext {
    use lazyos_ui::Glyph;
    pub fn from_str(s: &str) -> Option<Glyph> {
        match s {
            "computer" => Some(Glyph::Desktop),
            "tag" => Some(Glyph::Tag),
            "chip" => Some(Glyph::Chip),
            "clock" => Some(Glyph::Clock),
            "box" => Some(Glyph::Box),
            "disk" => Some(Glyph::Disk),
            "battery" => Some(Glyph::Battery),
            "wifi" => Some(Glyph::Wifi),
            "eye" => Some(Glyph::Eye),
            "code" => Some(Glyph::Code),
            "bug" => Some(Glyph::Bug),
            "clipboard" => Some(Glyph::Clipboard),
            "terminal" => Some(Glyph::Terminal),
            _ => Some(Glyph::Info),
        }
    }
}

use glyph_ext::from_str;

/// `lazyos menu about` (`rofi-about`): the about screen.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(&[
        "wl-copy",
        "sudo",
        "journalctl",
        "systemd-analyze",
        "lspci",
        "lsusb",
        "ps",
        "nmcli",
        "uname",
        "uptime",
        "df",
        "sway",
    ]) {
        say(
            "About",
            format!(
                "{said} (coreutils, util-linux, systemd, pciutils, usbutils, procps, network-manager, sway)"
            ),
            lazyos_ui::Urgency::Critical,
            Glyph::Info,
        );
        return Ok(3);
    }
    let mut screen = AboutScreen::new();
    let code = crate::run_screen(terminal, design, "About", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_parsing() {
        let content = "commit abc123 ui sway kernel 7.2.7 built 2026-11-04";
        let parts: Vec<&str> = content.split_whitespace().collect();
        assert_eq!(parts.len(), 8);
        let rev = format!("{} · {} · {} · {}", parts[1], parts[3], parts[5], parts[7]);
        assert_eq!(rev, "abc123 · sway · 7.2.7 · 2026-11-04");
    }

    #[test]
    fn meminfo_parsing() {
        let content = "MemTotal: 4000000 kB\nMemAvailable: 2000000 kB\n";
        let mut total = 0;
        let mut available = 0;
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total = line.split_whitespace().nth(1).unwrap().parse().unwrap();
            } else if line.starts_with("MemAvailable:") {
                available = line.split_whitespace().nth(1).unwrap().parse().unwrap();
            }
        }
        assert_eq!(total, 4000000);
        assert_eq!(available, 2000000);
    }

    #[test]
    fn views_hold_and_alt_q_footer() {
        let v = views();
        let hold_count = v.iter().filter(|(_, _, c)| c.contains("--hold")).count();
        let footer_count = v
            .iter()
            .filter(|(_, _, c)| c.contains("close this window with Alt+Q"))
            .count();
        assert_eq!(hold_count, 3);
        assert_eq!(footer_count, 3);
    }
}
