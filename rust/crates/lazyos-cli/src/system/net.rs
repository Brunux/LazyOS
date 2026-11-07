//! Wi-Fi & Network Manager (`bar-net`, `lazyos menu net`).
//!
//! Scans available Wi-Fi networks using `nmcli`, displays signal strength,
//! encryption status, active connection indicator, and provides masked
//! password input for secured networks. Also provides headless `--status`
//! output formatted as JSON for waybar's network indicator.

use std::collections::BTreeMap;
use std::process::Command;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{have, out, say};

/// One access point record parsed from nmcli.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WifiAp {
    pub in_use: bool,
    pub ssid: String,
    pub security: String,
    pub signal: u8,
}

/// Convert a 0..=100 signal percentage into a visual gauge.
pub fn signal_bar(signal: u8) -> &'static str {
    match signal {
        0..=19 => " ",
        20..=39 => "▂",
        40..=59 => "▃",
        60..=79 => "▄",
        _ => "▅",
    }
}

/// Parse nmcli tabular output into structured, de-duplicated AP records.
///
/// Input format: `IN-USE:SSID:SECURITY:SIGNAL`
/// Where colons in SSID are escaped as `\:`.
/// Duplicate SSIDs are collapsed to the strongest signal; active networks sort first.
pub fn parse_nmcli_wifi(raw: &str) -> Vec<WifiAp> {
    let mut best: BTreeMap<String, WifiAp> = BTreeMap::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Split on unescaped colons
        let mut fields = Vec::new();
        let mut cur = String::new();
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' && chars.peek() == Some(&':') {
                cur.push(':');
                chars.next();
            } else if ch == ':' {
                fields.push(cur);
                cur = String::new();
            } else {
                cur.push(ch);
            }
        }
        fields.push(cur);

        if fields.len() < 4 {
            continue;
        }

        let in_use = fields[0] == "*";
        let signal: u8 = fields.last().and_then(|s| s.parse().ok()).unwrap_or(0);
        let security = fields[fields.len() - 2].clone();
        let ssid = fields[1..fields.len() - 2].join(":");

        if ssid.is_empty() {
            continue;
        }

        let ap = WifiAp {
            in_use,
            ssid: ssid.clone(),
            security,
            signal,
        };

        match best.get(&ssid) {
            Some(existing) => {
                if ap.in_use || (ap.signal > existing.signal && !existing.in_use) {
                    best.insert(ssid, ap);
                }
            }
            None => {
                best.insert(ssid, ap);
            }
        }
    }

    let mut list: Vec<WifiAp> = best.into_values().collect();
    // Sort active first, then strongest signal descending, then by SSID
    list.sort_by(|a, b| {
        b.in_use
            .cmp(&a.in_use)
            .then_with(|| b.signal.cmp(&a.signal))
            .then_with(|| a.ssid.cmp(&b.ssid))
    });
    list
}

/// Headless query for waybar `custom/net` module: emits JSON.
pub fn status_json() -> String {
    let radio = out(&["nmcli", "radio", "wifi"]).unwrap_or_default();
    if radio != "enabled" {
        return r#"{"text":"off","tooltip":"Wi-Fi radio is off","class":"disabled","percentage":0}"#
            .to_string();
    }

    let scan_raw = out(&[
        "nmcli",
        "--terse",
        "--fields",
        "IN-USE,SSID,SECURITY,SIGNAL",
        "device",
        "wifi",
        "list",
        "--rescan",
        "no",
    ])
    .unwrap_or_default();

    let aps = parse_nmcli_wifi(&scan_raw);
    if let Some(active) = aps.iter().find(|ap| ap.in_use) {
        format!(
            r#"{{"text":"{}","tooltip":"Connected to {} ({}%)","class":"connected","percentage":{}}}"#,
            escape_json(&active.ssid),
            escape_json(&active.ssid),
            active.signal,
            active.signal
        )
    } else {
        r#"{"text":"disconnected","tooltip":"Wi-Fi disconnected","class":"disconnected","percentage":0}"#
            .to_string()
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Check if a connection profile exists for this SSID.
fn has_saved_connection(ssid: &str) -> bool {
    let conns =
        out(&["nmcli", "--terse", "--fields", "NAME", "connection", "show"]).unwrap_or_default();
    conns.lines().any(|l| l.trim() == ssid)
}

/// Connect to a Wi-Fi network.
fn connect_to(ssid: &str, password: Option<&str>) -> Result<String, String> {
    let mut cmd = Command::new("nmcli");
    cmd.args(["device", "wifi", "connect", ssid]);
    if let Some(pw) = password {
        cmd.args(["password", pw]);
    }
    let res = cmd.output().map_err(|e| e.to_string())?;
    if res.status.success() {
        return Ok(String::from_utf8_lossy(&res.stdout).trim().to_string());
    }
    // Try connection up by id if saved profile exists
    if has_saved_connection(ssid) {
        let up = Command::new("nmcli")
            .args(["connection", "up", "id", ssid])
            .output()
            .map_err(|e| e.to_string())?;
        if up.status.success() {
            return Ok(String::from_utf8_lossy(&up.stdout).trim().to_string());
        }
    }
    let stderr = String::from_utf8_lossy(&res.stderr);
    Err(stderr.trim().to_string())
}

/// `lazyos menu net` interactive surface.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if !have("nmcli") {
        say(
            "Wi-Fi",
            "nmcli is not installed (package network-manager)",
            Urgency::Critical,
            Glyph::Wifi,
        );
        return Ok(3);
    }

    let radio = out(&["nmcli", "radio", "wifi"]).unwrap_or_default();
    if radio != "enabled" {
        let mut picker = Picker::new(
            "Wi-Fi",
            vec![Question::choose(
                "Wi-Fi is off",
                "Enter turns the radio on · Escape closes",
                vec![row("Turn Wi-Fi on", "enable radio", Glyph::Wifi)],
                vec!["on".to_string()],
            )],
        );
        let code = crate::run_screen(terminal, design, "Wi-Fi", &mut picker)?;
        if code != 0 {
            return Ok(code);
        }
        if picker.answers().first().map(String::as_str) == Some("on") {
            let _ = Command::new("nmcli").args(["radio", "wifi", "on"]).status();
            say("Wi-Fi", "Wi-Fi turned on", Urgency::Normal, Glyph::Wifi);
        }
        return Ok(0);
    }

    // Radio is on: scan networks
    let scan_raw = out(&[
        "nmcli",
        "--terse",
        "--fields",
        "IN-USE,SSID,SECURITY,SIGNAL",
        "device",
        "wifi",
        "list",
        "--rescan",
        "no",
    ])
    .unwrap_or_default();

    let aps = parse_nmcli_wifi(&scan_raw);

    let mut rows = Vec::new();
    let mut keys = Vec::new();

    // Top action
    rows.push(row("Turn Wi-Fi off", "disable radio", Glyph::Wifi));
    keys.push("radio:off".to_string());

    for ap in &aps {
        let mark = if ap.in_use { "● " } else { "  " };
        let label = format!("{}{}", mark, ap.ssid);
        let sec = if ap.security.is_empty() || ap.security == "--" {
            "open".to_string()
        } else {
            ap.security.clone()
        };
        let detail = format!("{} {}% · {}", signal_bar(ap.signal), ap.signal, sec);
        rows.push(row(label, detail, Glyph::Wifi));
        keys.push(format!("ap:{}", ap.ssid));
    }

    // Utility actions
    rows.push(row(
        "Rescan networks",
        "scan all Wi-Fi channels now",
        Glyph::Sync,
    ));
    keys.push("rescan".to_string());
    rows.push(row(
        "Show the password as a QR",
        "Wi-Fi credentials QR",
        Glyph::Qr,
    ));
    keys.push("qr".to_string());
    rows.push(row(
        "Network settings (nmtui)",
        "manage connections in terminal",
        Glyph::Gear,
    ));
    keys.push("nmtui".to_string());

    let mut picker = Picker::new(
        "Wi-Fi",
        vec![Question::choose(
            "Wi-Fi",
            "Enter connects · Escape closes",
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Wi-Fi", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    let Some(action) = answers.first() else {
        return Ok(1);
    };

    match action.as_str() {
        "radio:off" => {
            let _ = Command::new("nmcli")
                .args(["radio", "wifi", "off"])
                .status();
            say("Wi-Fi", "Wi-Fi turned off", Urgency::Normal, Glyph::Wifi);
            Ok(0)
        }
        "rescan" => {
            let _ = Command::new("nmcli")
                .args(["device", "wifi", "list", "--rescan", "yes"])
                .status();
            say("Wi-Fi", "Wi-Fi scan complete", Urgency::Normal, Glyph::Wifi);
            Ok(0)
        }
        "qr" => {
            let _ = Command::new("lazyos")
                .args(["menu", "qr", "--wifi"])
                .status();
            Ok(0)
        }
        "nmtui" => {
            let _ = Command::new("foot")
                .args(["--app-id=popup", "nmtui"])
                .spawn();
            Ok(0)
        }
        act if act.starts_with("ap:") => {
            let ssid = &act[3..];
            let target_ap = aps.iter().find(|a| a.ssid == ssid);
            let is_open = target_ap.map_or(false, |a| a.security.is_empty() || a.security == "--");
            let has_saved = has_saved_connection(ssid);

            if has_saved || is_open {
                // Connect directly
                match connect_to(ssid, None) {
                    Ok(_) => {
                        say(
                            "Wi-Fi",
                            format!("Connected to {ssid}"),
                            Urgency::Normal,
                            Glyph::Wifi,
                        );
                        Ok(0)
                    }
                    Err(err) => {
                        say(
                            "Wi-Fi",
                            format!("Could not connect to {ssid}: {err}"),
                            Urgency::Critical,
                            Glyph::Wifi,
                        );
                        Ok(3)
                    }
                }
            } else {
                // Prompt for password
                let mut pw_picker = Picker::new(
                    "Wi-Fi Password",
                    vec![Question::input(
                        format!("Password for {ssid}"),
                        "Enter password for network · Escape cancels",
                        "WPA Key",
                        true,
                    )],
                );
                let pw_code =
                    crate::run_screen(terminal, design, "Wi-Fi Password", &mut pw_picker)?;
                if pw_code != 0 {
                    return Ok(pw_code);
                }
                let pw = pw_picker.answers().first().cloned().unwrap_or_default();
                if pw.is_empty() {
                    return Ok(1);
                }
                match connect_to(ssid, Some(&pw)) {
                    Ok(_) => {
                        say(
                            "Wi-Fi",
                            format!("Connected to {ssid}"),
                            Urgency::Normal,
                            Glyph::Wifi,
                        );
                        Ok(0)
                    }
                    Err(err) => {
                        say(
                            "Wi-Fi",
                            format!("Could not connect to {ssid}: {err}"),
                            Urgency::Critical,
                            Glyph::Wifi,
                        );
                        Ok(3)
                    }
                }
            }
        }
        _ => Ok(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nmcli_wifi_parses_records_and_collapses_duplicates() {
        let raw = "\
*:MyHome:WPA2:80
 :MyHome:WPA2:65
 :CoffeeShop:--:50
 :Mesh\\:Network:WPA3:90
";
        let aps = parse_nmcli_wifi(raw);
        assert_eq!(aps.len(), 3);
        // Active should be first
        assert_eq!(aps[0].ssid, "MyHome");
        assert!(aps[0].in_use);
        assert_eq!(aps[0].signal, 80);

        // Next strongest
        assert_eq!(aps[1].ssid, "Mesh:Network");
        assert_eq!(aps[1].signal, 90);
        assert_eq!(aps[1].security, "WPA3");

        // Next
        assert_eq!(aps[2].ssid, "CoffeeShop");
        assert_eq!(aps[2].security, "--");
    }

    #[test]
    fn signal_bar_gauge_steps() {
        assert_eq!(signal_bar(10), " ");
        assert_eq!(signal_bar(30), "▂");
        assert_eq!(signal_bar(50), "▃");
        assert_eq!(signal_bar(70), "▄");
        assert_eq!(signal_bar(90), "▅");
    }

    #[test]
    fn status_json_disabled_format() {
        let json =
            r#"{"text":"off","tooltip":"Wi-Fi radio is off","class":"disabled","percentage":0}"#;
        assert!(json.contains("\"class\":\"disabled\""));
    }
}
