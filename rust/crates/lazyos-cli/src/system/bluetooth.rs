//! Bluetooth picker (`bar-bt`): scan, connect, disconnect, bluetoothctl.
//!
//! The old script scanned for 3 seconds, listed known devices with connected
//! ones first, and offered connect/disconnect, turn off, and open bluetoothctl.

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

const NEEDED: &[&str] = &["bluetoothctl", "foot"];

/// The bluetooth screen.
pub struct BluetoothScreen {
    devices: Vec<Device>,
    powered: bool,
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
        let mut screen = Self {
            keys: Vec::new(),
            devices: Vec::new(),
            powered: false,
            list: ListState::new(),
            header: Header::new("LazyOS").subtitle("Bluetooth"),
            keybar: vec![
                KeyHint::new("Enter", "connect/disconnect"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
        };
        screen.reload();
        screen
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

        // Get all known devices
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
        // Connected first, then alphabetical
        devices.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| a.name.cmp(&b.name))
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
        } else if self.devices.is_empty() {
            rows.push(
                Row::new("No devices found")
                    .glyph(Glyph::Bluetooth)
                    .detail("scan to discover"),
            );
        } else {
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
        }
        if self.powered {
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
        } else {
            ctx.buf.set_string(
                filter.x,
                filter.y,
                "Enter connects/disconnects · footer rows for power off and bluetoothctl",
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

impl BluetoothScreen {}

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
            "power-on" => {
                let _ = std::process::Command::new("bluetoothctl")
                    .args(["power", "on"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                say(
                    "Bluetooth",
                    "Bluetooth on",
                    lazyos_ui::Urgency::Normal,
                    Glyph::Bluetooth,
                );
                self.reload();
            }
            "power-off" => {
                let _ = std::process::Command::new("bluetoothctl")
                    .args(["power", "off"])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                say(
                    "Bluetooth",
                    "Bluetooth off",
                    lazyos_ui::Urgency::Normal,
                    Glyph::Bluetooth,
                );
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
                if let Some(d) = self.devices.iter().find(|d| d.mac == mac) {
                    if d.connected {
                        let _ = std::process::Command::new("bluetoothctl")
                            .args(["disconnect", mac])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                        say(
                            "Bluetooth",
                            format!("Disconnected {}", d.name),
                            lazyos_ui::Urgency::Normal,
                            Glyph::Bluetooth,
                        );
                    } else {
                        let _ = std::process::Command::new("bluetoothctl")
                            .args(["connect", mac])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .status();
                        say(
                            "Bluetooth",
                            format!("Connected to {}", d.name),
                            lazyos_ui::Urgency::Normal,
                            Glyph::Bluetooth,
                        );
                    }
                    self.reload();
                }
            }
            _ => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bluetooth_needed_tools() {
        assert!(NEEDED.contains(&"bluetoothctl"));
        assert!(NEEDED.contains(&"foot"));
    }
}
