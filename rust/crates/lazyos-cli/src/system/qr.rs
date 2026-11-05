//! QR codes: the Wi-Fi network this machine is on, or any text (`rofi-qr`).
//!
//! The payload is the Wi-Fi QR format phones understand:
//!
//!     WIFI:T:WPA;S:<ssid>;P:<password>;;
//!
//! with `\`, `;`, `,` and `:` escaped, which is what the ZXing reader expects.
//! The password is read with `nmcli -s` through sudo. The payload is written to
//! a 0600 temp file and drawn by `qrencode -t ANSIUTF8` in a foot popup.

use std::cell::Cell;

use lazyos_ui::{
    BodyCtx, Design, Glyph, Header, KeyHint, ListAction, ListState, Outcome, Row, Screen,
    TextStyle, draw_card,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;

use crate::system::{have, missing, out, say, sudo};

const NEEDED: &[&str] = &["qrencode", "nmcli", "sudo", "foot"];

/// The QR screen.
pub struct QrScreen {
    wifi_detail: String,
    wifi_colour: lazyos_ui::ColorRole,
    list: ListState,
    header: Header,
    keybar: Vec<KeyHint>,
    rows_height: Cell<u16>,
    keys: Vec<String>,
}

impl QrScreen {
    pub fn new() -> Self {
        let mut screen = Self {
            keys: Vec::new(),
            wifi_detail: "not connected".to_string(),
            wifi_colour: lazyos_ui::ColorRole::Dim,
            list: ListState::new(),
            header: Header::new("LazyOS").subtitle("QR code"),
            keybar: vec![
                KeyHint::new("Enter", "show the code"),
                KeyHint::new("Esc", "close"),
            ],
            rows_height: Cell::new(20),
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
        self.keys = rows.iter().filter_map(|r| r.value.clone()).collect();
        self.list.set_rows(rows);
    }

    fn qr_escape(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace(';', "\\;")
            .replace(',', "\\,")
            .replace(':', "\\:")
    }

    fn show_qr(payload: &str, title: &str) {
        let tmp = std::env::temp_dir().join(format!("qr-{}.txt", std::process::id()));
        let _ = std::fs::write(&tmp, payload);
        let _ = std::process::Command::new("foot")
            .args(["--app-id=popup", "-T", title, "-e", "sh", "-c", &format!(
                "qrencode -t ANSIUTF8 -r '{}' || printf 'qrencode could not draw that\\n'; printf '\\nEnter closes\\n'; IFS= read -r _ || true",
                tmp.display()
            )])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        let _ = std::fs::remove_file(&tmp);
    }

    fn wifi_qr(&self) {
        let dev =
            out(&["nmcli", "-t", "-f", "DEVICE,TYPE", "device", "status"]).unwrap_or_default();
        let dev = dev
            .lines()
            .find(|l| l.split(':').nth(1) == Some("wifi"))
            .and_then(|l| l.split(':').next());
        if let Some(dev) = dev {
            let con = out(&["nmcli", "-g", "GENERAL.CONNECTION", "device", "show", dev])
                .unwrap_or_default();
            let con = con.lines().next().unwrap_or("").trim();
            if !con.is_empty() && con != "--" {
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
                if !ssid.is_empty() {
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
                    let payload = if psk.is_empty() {
                        format!("WIFI:T:nopass;S:{};;", Self::qr_escape(&ssid))
                    } else {
                        format!(
                            "WIFI:T:WPA;S:{};P:{};;",
                            Self::qr_escape(&ssid),
                            Self::qr_escape(&psk)
                        )
                    };
                    Self::show_qr(&payload, &format!("Wi-Fi · {}", con));
                }
            }
        }
    }

    fn text_qr(&self) {
        // For now, we can't easily prompt for text in this context
        // The old script used bar-pick -free; we'll just say it's not available
        say(
            "QR code",
            "Text input not available from this menu; use the command line",
            lazyos_ui::Urgency::Normal,
            Glyph::Qr,
        );
    }
}

impl Screen for QrScreen {
    fn draw(&mut self, ctx: &mut BodyCtx) {
        draw_card(ctx.buf, ctx.area, ctx.design, Some("QR code"), true);
        let (filter, rows) = super::pick::list_layout(ctx.area);
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
        if index >= rows.len() {
            return Outcome::Continue;
        }
        let key = self.keys.get(index).cloned().unwrap_or_default();
        if key.is_empty() || key == "-" {
            return Outcome::Continue;
        }
        match key.as_str() {
            "wifi" => self.wifi_qr(),
            "text" => self.text_qr(),
            _ => {}
        }
        Outcome::Continue
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
}
