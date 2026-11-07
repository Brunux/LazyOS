//! QR codes: the Wi-Fi network this machine is on, or any text (`rofi-qr`).
//!
//! The payload is the Wi-Fi QR format phones understand:
//!
//! ```text
//! WIFI:T:WPA;S:<ssid>;P:<password>;;
//! ```
//!
//! with `\`, `;`, `,` and `:` escaped, which is what the ZXing reader expects.
//! The password is read with `nmcli -s` through sudo. The payload is written to
//! a 0600 temp file and drawn by `qrencode -t ANSIUTF8` in a foot popup.

#![allow(unused_imports)]
#![allow(dead_code)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::vec_init_then_push)]
use std::cell::Cell;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, Input, InputAction, KeyHint, ListAction, ListState, Outcome,
    Row, Screen, TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::system::{missing, out, say, sudo};

const NEEDED: &[&str] = &["qrencode", "nmcli", "sudo", "foot"];

/// The QR screen.
pub struct QrScreen {
    wifi_detail: String,
    wifi_colour: lazyos_ui::ColorRole,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    typing: Option<Input>,
}

/// Format a Wi-Fi QR payload (`WIFI:T:WPA;S:...;P:...;;` or `nopass`).
pub fn format_wifi_payload(ssid: &str, psk: &str) -> String {
    if psk.is_empty() {
        format!("WIFI:T:nopass;S:{};;", QrScreen::qr_escape(ssid))
    } else {
        format!(
            "WIFI:T:WPA;S:{};P:{};;",
            QrScreen::qr_escape(ssid),
            QrScreen::qr_escape(psk)
        )
    }
}

/// Read the connected Wi-Fi network's QR payload and window title.
pub fn payload_wifi() -> Option<(String, String)> {
    let dev = out(&["nmcli", "-t", "-f", "DEVICE,TYPE", "device", "status"])?;
    let dev = dev
        .lines()
        .find(|l| l.split(':').nth(1) == Some("wifi"))
        .and_then(|l| l.split(':').next())?;
    let con = out(&["nmcli", "-g", "GENERAL.CONNECTION", "device", "show", dev])?;
    let con = con.lines().next().unwrap_or("").trim();
    if con.is_empty() || con == "--" {
        return None;
    }
    let ssid = out(&[
        "nmcli",
        "-g",
        "802-11-wireless.ssid",
        "connection",
        "show",
        con,
    ])
    .unwrap_or_default()
    .trim()
    .to_string();
    if ssid.is_empty() {
        return None;
    }
    let psk = sudo(&[
        "nmcli",
        "-s",
        "-g",
        "802-11-wireless-security.psk",
        "connection",
        "show",
        con,
    ])
    .unwrap_or_default()
    .trim()
    .to_string();
    Some((format_wifi_payload(&ssid, &psk), format!("Wi-Fi · {}", con)))
}

/// Write `payload` to a 0600 temp file and draw it with `qrencode` in a `foot` popup.
pub fn show(payload: &str, title: &str) {
    let tmp = std::env::temp_dir().join(format!("qr-{}.txt", std::process::id()));
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
        {
            let _ = f.write_all(payload.as_bytes());
        }
    }
    let escaped_title = title.replace('\'', "'\\''");
    let escaped_tmp = tmp.display().to_string().replace('\'', "'\\''");
    let script = format!(
        "exec foot --app-id=popup -T '{escaped_title}' -e sh -c 'qrencode -t ANSIUTF8 -r \"$1\" || printf \"qrencode could not draw that\\n\"; rm -f \"$1\"; printf \"\\nEnter closes\\n\"; IFS= read -r _ || true' sh '{escaped_tmp}'"
    );
    crate::system::spawn_detached(&script);
}

/// `lazyos menu qr --wifi` (`rofi-qr --wifi`): show the current Wi-Fi QR directly.
pub fn wifi_dialog() -> i32 {
    if let Some((payload, title)) = payload_wifi() {
        show(&payload, &title);
        crate::EXIT_OK
    } else {
        say(
            "QR code",
            "Not connected to a Wi-Fi network",
            lazyos_ui::Urgency::Normal,
            Glyph::Wifi,
        );
        crate::EXIT_FAILED
    }
}

impl QrScreen {
    pub fn new() -> Self {
        let mut screen = Self {
            wifi_detail: "not connected".to_string(),
            wifi_colour: lazyos_ui::ColorRole::Dim,
            list: ListState::new(),
            header: Header::new("LazyOS").subtitle("QR code"),
            keybar: vec![
                KeyHint::new("Enter", "show the code"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
            typing: None,
        };
        screen.reload();
        screen
    }

    fn reload(&mut self) {
        let (detail, colour) = Self::get_wifi_status();
        self.wifi_detail = detail;
        self.wifi_colour = colour;
        self.build_rows();
    }

    fn get_wifi_status() -> (String, lazyos_ui::ColorRole) {
        let dev =
            out(&["nmcli", "-t", "-f", "DEVICE,TYPE", "device", "status"]).unwrap_or_default();
        let dev = dev
            .lines()
            .find(|l| l.split(':').nth(1) == Some("wifi"))
            .and_then(|l| l.split(':').next())
            .map(|s| s.to_string());
        if let Some(dev) = dev {
            let con = out(&["nmcli", "-g", "GENERAL.CONNECTION", "device", "show", &dev])
                .unwrap_or_default();
            let con = con.lines().next().unwrap_or("").trim();
            if !con.is_empty() && con != "--" {
                return (con.to_string(), lazyos_ui::ColorRole::Ok);
            }
        }
        ("not connected".to_string(), lazyos_ui::ColorRole::Dim)
    }

    fn build_rows(&mut self) {
        let mut rows = Vec::new();
        rows.push(
            Row::new("Wi-Fi network")
                .glyph(Glyph::Wifi)
                .detail(self.wifi_detail.clone())
                .value("wifi"),
        );
        rows.push(
            Row::new("Any text")
                .glyph(Glyph::Qr)
                .detail("type it, and it becomes a code")
                .value("text"),
        );
        self.list.set_rows(rows);
    }

    fn qr_escape(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace(';', "\\;")
            .replace(',', "\\,")
            .replace(':', "\\:")
    }

    fn show_qr(payload: &str, title: &str) {
        show(payload, title);
    }

    fn wifi_qr(&self) {
        if let Some((payload, title)) = payload_wifi() {
            Self::show_qr(&payload, &title);
        } else {
            say(
                "QR code",
                "Not connected to a Wi-Fi network",
                lazyos_ui::Urgency::Normal,
                Glyph::Wifi,
            );
        }
    }

    fn text_qr(&mut self) {
        self.typing = Some(Input::new());
    }
}

impl Screen for QrScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("QR code"), true);
        let (filter, rows) = super::pick::list_layout(ctx.area);
        if let Some(input) = &self.typing {
            ctx.buf.set_string(
                filter.x,
                filter.y,
                "Type text or URL · Enter shows the code · Escape returns",
                ctx.design.style(TextStyle::Dim),
            );
            let field = Rect {
                height: 1.min(rows.height),
                ..rows
            };
            input.render(field, ctx.buf, ctx.design, "text or URL…");
            return;
        }
        ctx.buf.set_string(
            filter.x,
            filter.y,
            "Enter shows the code · Escape closes",
            ctx.design.style(TextStyle::Dim),
        );
        self.rows_height.set(rows.height);
        self.list.render(rows, ctx.buf, ctx.design, true);
    }

    fn key(&mut self, key: KeyEvent, design: &Design) -> Outcome {
        if key.kind != KeyEventKind::Press {
            return Outcome::Continue;
        }
        if let Some(input) = &mut self.typing {
            match input.handle(key) {
                InputAction::Submitted => {
                    let trimmed = input.text().trim().to_string();
                    self.typing = None;
                    if !trimmed.is_empty() {
                        Self::show_qr(&trimmed, "QR code");
                        return Outcome::Done(lazyos_ui::app::exit::OK);
                    }
                    return Outcome::Consumed;
                }
                InputAction::Cancelled => {
                    self.typing = None;
                    return Outcome::Consumed;
                }
                _ => return Outcome::Consumed,
            }
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
        if self.typing.is_some() {
            return None;
        }
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

impl QrScreen {
    fn activate(&mut self, index: usize) -> Outcome {
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
        match key {
            "wifi" => {
                self.wifi_qr();
                Outcome::Done(lazyos_ui::app::exit::OK)
            }
            "text" => {
                self.text_qr();
                Outcome::Consumed
            }
            _ => Outcome::Continue,
        }
    }
}

/// `lazyos menu qr` (`rofi-qr`): the window.
pub fn menu(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "QR code",
            format!("{said} (qrencode, nmcli, sudo, foot)"),
            lazyos_ui::Urgency::Critical,
            Glyph::Qr,
        );
        return Ok(3);
    }
    let mut screen = QrScreen::new();
    let code = crate::run_screen(terminal, design, "QR code", &mut screen)?;
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_escape_chars() {
        assert_eq!(QrScreen::qr_escape("a\\b;c,d:e"), "a\\\\b\\;c\\,d\\:e");
        assert_eq!(QrScreen::qr_escape("plain"), "plain");
    }

    #[test]
    fn wifi_payload_formats_wpa_and_nopass() {
        assert_eq!(
            format_wifi_payload("My:Net;1", "pa,ss\\2"),
            "WIFI:T:WPA;S:My\\:Net\\;1;P:pa\\,ss\\\\2;;"
        );
        assert_eq!(
            format_wifi_payload("OpenNet", ""),
            "WIFI:T:nopass;S:OpenNet;;"
        );
    }

    #[test]
    fn text_row_opens_input_and_esc_returns_to_list() {
        let mut screen = QrScreen::new();
        screen.build_rows();
        assert_eq!(screen.activate(1), Outcome::Consumed);
        assert!(screen.typing.is_some());
        let design = Design::solarized();
        let esc = KeyEvent::from(KeyCode::Esc);
        assert_eq!(screen.key(esc, &design), Outcome::Consumed);
        assert!(screen.typing.is_none());
    }
}
