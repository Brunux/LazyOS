//! Bluetooth picker (`bar-bt`): scan, connect, disconnect, bluetoothctl.
//!
//! Scans for nearby devices asynchronously, lists known devices with connected ones first,
//! prioritizes friendly human names over raw BLE beacons, and offers
//! connect/disconnect, power toggle, background scanning ('s' key), and bluetoothctl.

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
use std::cell::Cell;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::system::{busy, busy_end, busy_fail, missing, out, say};

const NEEDED: &[&str] = &["bluetoothctl", "foot"];

/// Returns true if the device name is just a formatted MAC address (e.g. "44-F9-B4-7F-6C-CA").
fn is_raw_mac(name: &str) -> bool {
    let clean = name.replace([':', '-'], "");
    clean.len() == 12 && clean.chars().all(|c| c.is_ascii_hexdigit())
}

/// Run an active scan for discovery using bluetoothctl.
fn do_scan(secs: u32) {
    let _ = std::process::Command::new("bluetoothctl")
        .args(["--timeout", &secs.to_string(), "scan", "on"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

/// The bluetooth screen.
pub struct BluetoothScreen {
    devices: Vec<Device>,
    powered: bool,
    scanning: bool,
    scan_rx: Option<Receiver<Vec<Device>>>,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    keys: Vec<String>,
}

#[derive(Clone, Debug)]
struct Device {
    mac: String,
    name: String,
    connected: bool,
}

impl BluetoothScreen {
    pub fn new() -> Self {
        let powered = Self::get_powered();
        let devices = if powered {
            Self::get_devices()
        } else {
            Vec::new()
        };
        let mut screen = Self {
            keys: Vec::new(),
            devices,
            powered,
            scanning: false,
            scan_rx: None,
            list: ListState::new(),
            header: Header::new("LazyOS").subtitle("Bluetooth"),
            keybar: vec![
                KeyHint::new("Enter", "select"),
                KeyHint::new("s", "scan"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
        };

        // If powered on, start a background scan immediately so newly
        // advertising devices appear automatically without blocking the UI.
        if powered {
            screen.start_scan(3);
        } else {
            screen.build_rows();
        }

        screen
    }

    fn start_scan(&mut self, secs: u32) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        let (tx, rx) = channel();
        self.scan_rx = Some(rx);
        std::thread::spawn(move || {
            do_scan(secs);
            let devs = BluetoothScreen::get_devices();
            let _ = tx.send(devs);
        });
        self.build_rows();
    }

    fn reload(&mut self) {
        self.powered = Self::get_powered();
        self.devices = if self.powered {
            Self::get_devices()
        } else {
            Vec::new()
        };
        self.build_rows();
    }

    fn get_powered() -> bool {
        out(&["bluetoothctl", "show"])
            .map(|s| s.contains("Powered: yes"))
            .unwrap_or(false)
    }

    fn get_devices() -> Vec<Device> {
        // Get connected devices first
        let connected = out(&["bluetoothctl", "devices", "Connected"]).unwrap_or_default();
        let mut connected_macs = std::collections::HashSet::new();
        for line in connected.lines() {
            if let Some(mac) = line
                .strip_prefix("Device ")
                .and_then(|s| s.split_whitespace().next())
            {
                connected_macs.insert(mac.to_string());
            }
        }

        // Get all known/discovered devices
        let known = out(&["bluetoothctl", "devices"]).unwrap_or_default();
        let mut devices = Vec::new();
        for line in known.lines() {
            if let Some(rest) = line.strip_prefix("Device ") {
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                if parts.len() == 2 {
                    let mac = parts[0].to_string();
                    let name = parts[1].to_string();
                    devices.push(Device {
                        mac: mac.clone(),
                        name,
                        connected: connected_macs.contains(&mac),
                    });
                }
            }
        }

        // Ordering:
        // 1. Connected devices first
        // 2. Named friendly devices before raw MAC addresses
        // 3. Alphabetical by name
        devices.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| is_raw_mac(&a.name).cmp(&is_raw_mac(&b.name)))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        devices
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        if !self.powered {
            rows.push(
                Row::new("Turn Bluetooth on")
                    .glyph(Glyph::Bluetooth)
                    .detail("power on the controller")
                    .value("power-on"),
            );
        } else {
            // Devices list
            for d in &self.devices {
                let label = if d.connected {
                    format!("* {} · connected", d.name)
                } else {
                    format!("  {}", d.name)
                };
                rows.push(
                    Row::new(label)
                        .glyph(Glyph::Bluetooth)
                        .detail(d.mac.clone())
                        .value(format!("device:{}", d.mac)),
                );
            }

            // Scan action row
            let (scan_label, scan_detail) = if self.scanning {
                (
                    "Scanning for devices…",
                    "discovery in progress · please wait",
                )
            } else if self.devices.is_empty() {
                (
                    "Scan for devices",
                    "no devices found · press Enter or 's' to scan",
                )
            } else {
                (
                    "Scan for devices",
                    "discover nearby Bluetooth devices (or press 's')",
                )
            };

            rows.push(
                Row::new(scan_label)
                    .glyph(Glyph::Sync)
                    .detail(scan_detail)
                    .value("scan"),
            );

            // Power off and helper tools
            rows.push(
                Row::new("Turn Bluetooth off")
                    .glyph(Glyph::Bluetooth)
                    .detail("power off the controller")
                    .value("power-off"),
            );
            rows.push(
                Row::new("Open bluetoothctl")
                    .glyph(Glyph::Terminal)
                    .detail("pairing keys, trust, remove, agent")
                    .value("bluetoothctl"),
            );
        }

        self.keys = rows.iter().filter_map(|r| r.value.clone()).collect();
        self.list.set_rows(rows);
    }
}

impl Screen for BluetoothScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("Bluetooth"), true);
        let (filter, rows) = super::pick::list_layout(ctx.area);
        if !self.powered {
            ctx.buf.set_string(
                filter.x,
                filter.y,
                "Bluetooth is off · Enter turns it on",
                ctx.design.style(TextStyle::Dim),
            );
        } else if self.scanning {
            ctx.buf.set_string(
                filter.x,
                filter.y,
                "Scanning for nearby Bluetooth devices… · Esc to close",
                ctx.design.style(TextStyle::Body),
            );
        } else {
            ctx.buf.set_string(
                filter.x,
                filter.y,
                "Enter connects/disconnects · 's' scans for nearby devices",
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
            KeyCode::Char('s') | KeyCode::Char('S') => {
                if self.powered && !self.scanning {
                    self.start_scan(3);
                    return Outcome::Continue;
                }
            }
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

    fn needs_tick(&self) -> Option<Duration> {
        if self.scanning {
            Some(Duration::from_millis(100))
        } else {
            None
        }
    }

    fn tick(&mut self) -> Outcome {
        if self.scanning {
            if let Some(rx) = &self.scan_rx {
                if let Ok(devs) = rx.try_recv() {
                    self.devices = devs;
                    self.scanning = false;
                    self.scan_rx = None;
                    self.build_rows();
                    say(
                        "Bluetooth",
                        if self.devices.is_empty() {
                            "No devices found".to_string()
                        } else {
                            format!("{} devices found", self.devices.len())
                        },
                        lazyos_ui::Urgency::Normal,
                        Glyph::Bluetooth,
                    );
                }
            }
        }
        Outcome::Continue
    }
}

/// The `--status` (`status`) JSON output for waybar (`custom/bt`).
pub fn status() -> String {
    let powered = BluetoothScreen::get_powered();
    if !powered {
        return format_status(false, &[]);
    }
    let connected = out(&["bluetoothctl", "devices", "Connected"]).unwrap_or_default();
    let names: Vec<String> = connected
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("Device ")?;
            let (_, name) = rest.split_once(' ')?;
            Some(name.trim().to_string())
        })
        .collect();
    format_status(true, &names)
}

fn format_status(powered: bool, connected_names: &[String]) -> String {
    const ICON: &str = "\u{f294}"; // Font Awesome bluetooth-b
    if !powered {
        return format!(
            "{{\"text\":\"<span font_desc='JetBrains Mono 13'>{}</span>\",\"class\":\"off\",\"tooltip\":\"Bluetooth off (click to turn on)\"}}",
            ICON
        );
    }
    if connected_names.is_empty() {
        format!(
            "{{\"text\":\"<span font_desc='JetBrains Mono 13'>{}</span>\",\"class\":\"on\",\"tooltip\":\"Bluetooth on · nothing connected\"}}",
            ICON
        )
    } else {
        let tooltip = connected_names.join("\\n").replace('"', "'");
        format!(
            "{{\"text\":\"<span font_desc='JetBrains Mono 13'>{}</span> {}\",\"class\":\"on\",\"tooltip\":\"{}\"}}",
            ICON,
            connected_names.len(),
            tooltip
        )
    }
}

/// `lazyos menu bluetooth` (`bar-bt`): the window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Bluetooth",
            format!("{said} (bluez, foot)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Bluetooth,
        );
        return Ok(3);
    }
    let mut screen = BluetoothScreen::new();
    let code = crate::run_screen(terminal, design, "Bluetooth", &mut screen)?;
    Ok(code)
}

impl BluetoothScreen {
    fn activate(&mut self, index: usize) -> Outcome {
        let rows = self.list.rows();
        if index >= rows.len() {
            return Outcome::Continue;
        }
        let key = self.keys.get(index).cloned().unwrap_or_default();
        if key.is_empty() || key == "-" {
            return Outcome::Continue;
        }
        match key.as_str() {
            "scan" => {
                if !self.scanning {
                    self.start_scan(3);
                }
            }
            "power-on" => {
                busy("bt", "Bluetooth", "Powering on…", "bluetooth");
                let _ = std::process::Command::new("bluetoothctl")
                    .args(["power", "on"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                busy_end("bt", "Bluetooth", "Bluetooth on");
                say(
                    "Bluetooth",
                    "Bluetooth on",
                    lazyos_ui::Urgency::Normal,
                    Glyph::Bluetooth,
                );
                self.reload();
                self.start_scan(3);
            }
            "power-off" => {
                busy("bt", "Bluetooth", "Powering off…", "bluetooth");
                let _ = std::process::Command::new("bluetoothctl")
                    .args(["power", "off"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                busy_end("bt", "Bluetooth", "Bluetooth off");
                say(
                    "Bluetooth",
                    "Bluetooth off",
                    lazyos_ui::Urgency::Normal,
                    Glyph::Bluetooth,
                );
                self.scanning = false;
                self.scan_rx = None;
                self.reload();
            }
            "bluetoothctl" => {
                let _ = std::process::Command::new("foot")
                    .args(["--app-id=popup", "bluetoothctl"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            k if k.starts_with("device:") => {
                let mac = &k[7..];
                if let Some(d) = self.devices.iter().find(|d| d.mac == mac).cloned() {
                    if d.connected {
                        busy(
                            "bt",
                            "Bluetooth",
                            &format!("Disconnecting {}…", d.name),
                            "bluetooth",
                        );
                        let out = std::process::Command::new("bluetoothctl")
                            .args(["disconnect", mac])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .output();
                        let text = out
                            .as_ref()
                            .map(|o| String::from_utf8_lossy(&o.stdout))
                            .unwrap_or_default();
                        if text.contains("Successful disconnected")
                            || out.as_ref().map(|o| o.status.success()).unwrap_or(false)
                        {
                            busy_end("bt", "Bluetooth", &format!("Disconnected {}", d.name));
                            say(
                                "Bluetooth",
                                format!("Disconnected {}", d.name),
                                lazyos_ui::Urgency::Normal,
                                Glyph::Bluetooth,
                            );
                        } else {
                            busy_fail("bt", "Bluetooth", "Disconnect failed", "dialog-error");
                            say(
                                "Bluetooth",
                                format!("Could not disconnect {}", d.name),
                                lazyos_ui::Urgency::Normal,
                                Glyph::Warning,
                            );
                        }
                    } else {
                        busy(
                            "bt",
                            "Bluetooth",
                            &format!("Connecting to {}…", d.name),
                            "bluetooth",
                        );
                        let out = std::process::Command::new("bluetoothctl")
                            .args(["connect", mac])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .output();
                        let text = out
                            .as_ref()
                            .map(|o| String::from_utf8_lossy(&o.stdout))
                            .unwrap_or_default();
                        if text.contains("Connection successful") {
                            busy_end("bt", "Bluetooth", &format!("Connected to {}", d.name));
                            say(
                                "Bluetooth",
                                format!("Connected to {}", d.name),
                                lazyos_ui::Urgency::Normal,
                                Glyph::Bluetooth,
                            );
                        } else {
                            // If direct connect failed, attempt pairing first
                            let pair_out = std::process::Command::new("bluetoothctl")
                                .args(["pair", mac])
                                .stdin(std::process::Stdio::null())
                                .stdout(std::process::Stdio::piped())
                                .stderr(std::process::Stdio::piped())
                                .output();
                            let pair_text = pair_out
                                .as_ref()
                                .map(|o| String::from_utf8_lossy(&o.stdout))
                                .unwrap_or_default();
                            if pair_text.contains("Pairing successful") {
                                let _ = std::process::Command::new("bluetoothctl")
                                    .args(["trust", mac])
                                    .stdin(std::process::Stdio::null())
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .status();
                                let _ = std::process::Command::new("bluetoothctl")
                                    .args(["connect", mac])
                                    .stdin(std::process::Stdio::null())
                                    .stdout(std::process::Stdio::null())
                                    .stderr(std::process::Stdio::null())
                                    .status();
                                busy_end(
                                    "bt",
                                    "Bluetooth",
                                    &format!("Paired and connected to {}", d.name),
                                );
                                say(
                                    "Bluetooth",
                                    format!("Connected to {}", d.name),
                                    lazyos_ui::Urgency::Normal,
                                    Glyph::Bluetooth,
                                );
                            } else {
                                busy_fail("bt", "Bluetooth", "Connection failed", "dialog-error");
                                say(
                                    "Bluetooth",
                                    format!("Could not connect to {}", d.name),
                                    lazyos_ui::Urgency::Normal,
                                    Glyph::Warning,
                                );
                            }
                        }
                    }
                    self.reload();
                }
            }
            _ => {}
        }
        Outcome::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_needed_tools() {
        assert!(NEEDED.contains(&"bluetoothctl"));
        assert!(NEEDED.contains(&"foot"));
    }

    #[test]
    fn raw_mac_detection() {
        assert!(is_raw_mac("44-F9-B4-7F-6C-CA"));
        assert!(is_raw_mac("44:F9:B4:7F:6C:CA"));
        assert!(!is_raw_mac("Sala de estar tv"));
        assert!(!is_raw_mac("AirPods"));
    }

    #[test]
    fn status_json_for_waybar() {
        let off = format_status(false, &[]);
        assert!(off.contains("\"class\":\"off\""));
        assert!(off.contains("\u{f294}"));
        let on = format_status(true, &["Buds".to_string()]);
        assert!(on.contains("\"class\":\"on\""));
        assert!(on.contains("</span> 1\""));
        assert!(on.contains("\"tooltip\":\"Buds\""));
    }
}
