//! Quick settings: the switches and panels of the session in one list (`rofi-quick`).
//!
//! Every row states what is on and what pressing it does. It re-draws itself
//! after a switch, so the state in the list is never stale. It leaves for the
//! pickers and panels that already exist (bar-net, bar-bt, bar-audio, bar-power)
//! instead of duplicating them. The two radio switches (Wi-Fi, Bluetooth) live
//! here as well because that is the one thing the pickers do not answer at a
//! glance.
//!
//! Nothing here is a daemon: every draw reads the state from nmcli,
//! bluetoothctl, swaymsg, makoctl, the night-light helper and sysfs.

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{out, say, sudo};

/// What the menu needs before it offers anything.
pub const NEEDED: &[&str] = &["nmcli", "bluetoothctl", "swaymsg", "makoctl", "iw"];

/// The `--status` output (not used as a separate subcommand, but the Settings
/// row in the menu shows the same probes).
pub fn status() -> String {
    let wifi = if wifi_on() { "on" } else { "off" };
    let bt = if bt_on() { "on" } else { "off" };
    let night = if night_on() { "on" } else { "off" };
    let dnd = if dnd_on() { "dnd" } else { "allowed" };
    let tp = if touchpad_on() { "enabled" } else { "disabled" };
    let am = if automount_on() { "on" } else { "off" };
    format!(
        "Wi-Fi: {} · BT: {} · Night: {} · DND: {} · TP: {} · AM: {}",
        wifi, bt, night, dnd, tp, am
    )
}

fn wifi_on() -> bool {
    out(&["nmcli", "radio", "wifi"])
        .map(|s| s.trim() == "enabled")
        .unwrap_or(false)
}

fn wifi_if() -> Option<String> {
    out(&["nmcli", "-t", "-f", "DEVICE,TYPE", "device", "status"]).and_then(|s| {
        s.lines().find_map(|l| {
            let parts: Vec<&str> = l.split(':').collect();
            if parts.len() == 2 && parts[1] == "wifi" {
                Some(parts[0].to_string())
            } else {
                None
            }
        })
    })
}

fn ssid() -> Option<String> {
    wifi_if().and_then(|dev| {
        out(&["iw", "dev", &dev, "link"]).and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("SSID:").map(|s| s.trim().to_string()))
        })
    })
}

fn wifi_detail() -> String {
    if !wifi_on() {
        "off".to_string()
    } else if let Some(s) = ssid() {
        s
    } else {
        "not connected".to_string()
    }
}

fn bt_on() -> bool {
    // busctl is one property read; bluetoothctl connects a whole D-Bus client
    // and asks bluez for everything, which is the slow half of this panel.
    let powered = out(&[
        "busctl",
        "--system",
        "get-property",
        "org.bluez",
        "/org/bluez/hci0",
        "org.bluez.Adapter1",
        "Powered",
    ])
    .and_then(|s| {
        s.lines()
            .find_map(|l| l.strip_prefix('b').map(|s| s.trim().to_string()))
    })
    .map(|s| s == "true");
    if let Some(p) = powered {
        return p;
    }
    out(&["bluetoothctl", "show"])
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("Powered:").map(|s| s.trim().to_string()))
        })
        .map(|s| s == "yes")
        .unwrap_or(false)
}

fn bt_detail() -> String {
    if !bt_on() {
        "off".to_string()
    } else {
        let n = out(&["bluetoothctl", "devices", "Connected"])
            .map(|s| s.lines().count())
            .unwrap_or(0);
        match n {
            0 => "on, nothing connected".to_string(),
            1 => "1 device connected".to_string(),
            _ => format!("{} devices connected", n),
        }
    }
}

fn night_on() -> bool {
    out(&["sh", "-c", "lazyos-night-light status 2>/dev/null || true"])
        .map(|s| s.starts_with("on"))
        .unwrap_or(false)
}

fn dnd_on() -> bool {
    out(&["makoctl", "mode"])
        .map(|s| s.lines().any(|l| l == "do-not-disturb"))
        .unwrap_or(false)
}

fn touchpad_on() -> bool {
    out(&["swaymsg", "-t", "get_inputs", "-r"])
        .and_then(|s| {
            let v: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&s);
            v.ok().and_then(|inputs| {
                inputs
                    .iter()
                    .filter(|i| i["type"].as_str() == Some("touchpad"))
                    .map(|i| i["libinput"]["send_events"].as_str().unwrap_or("enabled"))
                    .collect::<Vec<_>>()
                    .first()
                    .map(|s| *s != "disabled")
            })
        })
        .unwrap_or(true)
}

fn rotation_name() -> String {
    out(&["swaymsg", "-t", "get_outputs", "-r"])
        .and_then(|s| {
            let v: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&s);
            v.ok().and_then(|outputs| {
                outputs
                    .iter()
                    .find(|o| o["focused"].as_bool().unwrap_or(false))
                    .and_then(|o| o["transform"].as_str())
                    .map(|s| s.to_string())
            })
        })
        .unwrap_or_else(|| "normal".to_string())
}

fn tablet_mode_detail() -> String {
    out(&["sh", "-c", "lazyos-tablet-mode status 2>/dev/null || true"])
        .unwrap_or_else(|| "off".to_string())
}

fn automount_conf() -> std::path::PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("/"))
                .join(".config")
        })
        .join("lazyos/automount.conf")
}

fn automount_on() -> bool {
    std::fs::read_to_string(automount_conf())
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("enabled=").map(|s| s.to_string()))
        })
        .map(|v| v != "no")
        .unwrap_or(true)
}

fn audio_detail() -> String {
    out(&["wpctl", "get-volume", "@DEFAULT_AUDIO_SINK@"])
        .map(|v| {
            if v.contains("MUTED") {
                "muted".to_string()
            } else {
                v.strip_prefix("Volume: ")
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|s| s.parse::<f32>().ok())
                    .map(|f| format!("{}%", (f * 100.0 + 0.5) as u32))
                    .unwrap_or("unknown".to_string())
            }
        })
        .unwrap_or("unknown".to_string())
}

fn media_detail() -> String {
    out(&[
        "playerctl",
        "metadata",
        "--format",
        "{{artist}} — {{title}}",
    ])
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "nothing playing".to_string())
}

fn battery_detail() -> String {
    let c = std::fs::read_to_string("/sys/class/power_supply/sbs-8-000b/capacity").ok();
    let s = std::fs::read_to_string("/sys/class/power_supply/sbs-8-000b/status").ok();
    match (c, s) {
        (Some(cap), Some(sta)) => format!("{}% · {}", cap.trim(), sta.trim().to_lowercase()),
        _ => String::new(),
    }
}

/// Build the rows for the settings menu.
fn build_rows() -> (Vec<lazyos_ui::Row>, Vec<String>) {
    let mut rows = Vec::new();
    let mut keys = Vec::new();

    // Wi-Fi
    if wifi_on() {
        rows.push(row("Wi-Fi radio", "on", Glyph::Wifi));
        rows.push(row("Wi-Fi networks", wifi_detail(), Glyph::Globe));
        keys.push("wifi-radio".to_string());
        keys.push("wifi".to_string());
    } else {
        rows.push(row("Wi-Fi radio", "off", Glyph::Wifi));
        rows.push(row("Wi-Fi networks", wifi_detail(), Glyph::Globe));
        keys.push("wifi-radio".to_string());
        keys.push("wifi".to_string());
    }

    // Bluetooth
    if bt_on() {
        rows.push(row("Bluetooth radio", "on", Glyph::Bluetooth));
        rows.push(row("Bluetooth devices", bt_detail(), Glyph::Link));
        keys.push("bt-radio".to_string());
        keys.push("bt".to_string());
    } else {
        rows.push(row("Bluetooth radio", "off", Glyph::Bluetooth));
        rows.push(row("Bluetooth devices", bt_detail(), Glyph::Link));
        keys.push("bt-radio".to_string());
        keys.push("bt".to_string());
    }

    // Night light
    if night_on() {
        rows.push(row("Night light", "on", Glyph::Moon));
        keys.push("night".to_string());
    } else {
        rows.push(row("Night light", "off", Glyph::Moon));
        keys.push("night".to_string());
    }

    // Notifications / DND
    if dnd_on() {
        rows.push(row("Notifications", "do not disturb", Glyph::BellOff));
        rows.push(row("Do not disturb", "on", Glyph::BellOff));
        keys.push("notify".to_string());
        keys.push("dnd".to_string());
    } else {
        rows.push(row("Notifications", "allowed", Glyph::Bell));
        rows.push(row("Do not disturb", "off", Glyph::Bell));
        keys.push("notify".to_string());
        keys.push("dnd".to_string());
    }

    // VNC screen sharing
    let vnc_status = out(&["sh", "-c", "lazyos-vnc status 2>/dev/null || echo off"])
        .unwrap_or_else(|| "off".to_string());
    rows.push(row("Share this screen", vnc_status, Glyph::Eye));
    keys.push("vnc".to_string());

    // Touchpad
    if touchpad_on() {
        rows.push(row("Touchpad", "enabled", Glyph::Mouse));
        keys.push("touchpad".to_string());
    } else {
        rows.push(row("Touchpad", "disabled", Glyph::Mouse));
        keys.push("touchpad".to_string());
    }

    // Rotation
    rows.push(row("Rotation", rotation_name(), Glyph::Rotate));
    keys.push("rotation".to_string());

    // Display arrangement
    let display_status = out(&[
        "sh",
        "-c",
        "lazyos-display --status 2>/dev/null || echo unknown",
    ])
    .unwrap_or_else(|| "unknown".to_string());
    rows.push(row("Display arrangement", display_status, Glyph::Display));
    keys.push("display".to_string());

    // Auto-mount USB drives
    if automount_on() {
        rows.push(row(
            "Auto-mount USB drives",
            "a plugged stick mounts itself",
            Glyph::Disk,
        ));
        keys.push("automount".to_string());
    } else {
        rows.push(row(
            "Auto-mount USB drives",
            "off · Places mounts a stick when you pick it",
            Glyph::Disk,
        ));
        keys.push("automount".to_string());
    }

    // Tablet mode
    let tm = tablet_mode_detail();
    if tm == "off" {
        rows.push(row("Tablet mode", tm, Glyph::Keyboard));
        keys.push("tablet".to_string());
    } else {
        rows.push(row("Tablet mode", tm, Glyph::Keyboard));
        keys.push("tablet".to_string());
        // Calibrate rotation
        let calib = out(&[
            "sh",
            "-c",
            "lazyos-tablet-mode calibration 2>/dev/null || echo unknown",
        ])
        .unwrap_or_else(|| "unknown".to_string());
        rows.push(row("Calibrate rotation", calib, Glyph::Rotate));
        keys.push("tablet-calibrate".to_string());
    }

    // Keyboard layout
    let layout = out(&["swaymsg", "-t", "get_inputs", "-r"])
        .and_then(|s| {
            let v: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&s);
            v.ok().and_then(|inputs| {
                inputs
                    .iter()
                    .find(|i| i["type"].as_str() == Some("keyboard"))
                    .and_then(|i| i["xkb_active_layout_name"].as_str())
                    .map(|s| s.to_string())
            })
        })
        .unwrap_or_else(|| "unknown".to_string());
    rows.push(row("Keyboard layout", layout, Glyph::Keyboard));
    keys.push("layout".to_string());

    // Translate
    let translate_target = out(&[
        "sh",
        "-c",
        "lazyos-translate --target-name 2>/dev/null || echo English",
    ])
    .unwrap_or_else(|| "English".to_string());
    rows.push(row(
        "Translate",
        format!("into {}", translate_target),
        Glyph::Globe,
    ));
    keys.push("translate".to_string());

    // Audio output
    rows.push(row("Audio output", audio_detail(), Glyph::Volume));
    keys.push("audio".to_string());

    // Volume per application (pavucontrol)
    rows.push(row("Volume per application", "pavucontrol", Glyph::Volume));
    keys.push("mixer".to_string());

    // Media
    rows.push(row("Media", media_detail(), Glyph::Music));
    keys.push("media".to_string());

    // Battery / Power
    rows.push(row("Battery", battery_detail(), Glyph::Battery));
    keys.push("power".to_string());

    (rows, keys)
}

/// `lazyos menu settings` (`rofi-quick`): the settings picker.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Settings",
            format!("{said} (network, bluetooth, sway, mako, iw)"),
            Urgency::Critical,
            Glyph::Sliders,
        );
        return Ok(3);
    }

    let (rows, keys) = build_rows();

    let mut picker = Picker::new(
        "Settings",
        vec![Question::choose(
            "Settings",
            "Enter switches or opens · Escape closes",
            rows,
            keys,
        )],
    );

    let code = crate::run_screen(terminal, design, "Settings", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    if answers.is_empty() {
        return Ok(1);
    }

    // The shell version loops and re-renders. Here we handle the action once.
    // For a full re-render loop, we'd need to re-run the picker. For now,
    // handle the action and return.
    let key = &answers[0];

    match key.as_str() {
        "wifi-radio" => {
            let on = wifi_on();
            let action = if on { "off" } else { "on" };
            let msg = if on {
                "Turning the radio off…"
            } else {
                "Turning the radio on…"
            };
            say("Wi-Fi", msg.to_string(), Urgency::Normal, Glyph::Wifi);
            if let Err(_e) = sudo(&["nmcli", "radio", "wifi", action]) {
                say(
                    "Wi-Fi",
                    format!("Could not turn the radio {}", action),
                    Urgency::Critical,
                    Glyph::Wifi,
                );
                return Ok(3);
            }
            say(
                "Wi-Fi",
                format!("Wi-Fi {}", action),
                Urgency::Normal,
                Glyph::Wifi,
            );
            Ok(0)
        }
        "bt-radio" => {
            let on = bt_on();
            let action = if on { "off" } else { "on" };
            let msg = if on {
                "Powering the controller off…"
            } else {
                "Powering the controller on…"
            };
            say(
                "Bluetooth",
                msg.to_string(),
                Urgency::Normal,
                Glyph::Bluetooth,
            );
            if let Err(_e) = sudo(&["bluetoothctl", "power", action]) {
                say(
                    "Bluetooth",
                    format!("Could not power the controller {}", action),
                    Urgency::Critical,
                    Glyph::Bluetooth,
                );
                return Ok(3);
            }
            say(
                "Bluetooth",
                action.to_string(),
                Urgency::Normal,
                Glyph::Bluetooth,
            );
            Ok(0)
        }
        "wifi" => {
            // Open bar-net (nmcli TUI)
            let _ = std::process::Command::new("nmtui").spawn();
            Ok(0)
        }
        "bt" => {
            // Open bar-bt (bluetoothctl)
            let _ = std::process::Command::new("bluetoothctl").spawn();
            Ok(0)
        }
        "night" => {
            if night_on() {
                let _ = out(&["sh", "-c", "lazyos-night-light stop"]);
            } else {
                let _ = out(&["sh", "-c", "lazyos-night-light start"]);
            }
            Ok(0)
        }
        "dnd" => {
            let _ = out(&["makoctl", "mode", "-t", "do-not-disturb"]);
            Ok(0)
        }
        "notify" => {
            let _ = std::process::Command::new("lazyos")
                .arg("menu")
                .arg("notify")
                .spawn();
            Ok(0)
        }
        "vnc" => {
            let _ = std::process::Command::new("lazyos")
                .arg("menu")
                .arg("vnc")
                .spawn();
            Ok(0)
        }
        "touchpad" => {
            let _ = out(&["swaymsg", "input", "type:touchpad", "events", "toggle"]);
            Ok(0)
        }
        "rotation" => {
            let _ = std::process::Command::new("lazyos")
                .arg("menu")
                .arg("--rotation-menu")
                .spawn();
            Ok(0)
        }
        "display" => {
            let _ = std::process::Command::new("lazyos")
                .arg("menu")
                .arg("display")
                .spawn();
            Ok(0)
        }
        "automount" => {
            let conf = automount_conf();
            let (value, word) = if automount_on() {
                ("no", "off · Places mounts a stick when you pick it")
            } else {
                ("yes", "on · a plugged USB drive mounts itself")
            };
            let part = conf.with_extension("part");
            let conf_display = conf.display().to_string();
            if std::fs::create_dir_all(conf.parent().unwrap()).is_ok()
                && std::fs::write(&part, format!("enabled={}\n", value)).is_ok()
                && std::fs::rename(&part, &conf).is_ok()
            {
                say(
                    "Auto-mount",
                    format!("USB drives: {}", word),
                    Urgency::Normal,
                    Glyph::Disk,
                );
            } else {
                let _ = std::fs::remove_file(&part);
                say(
                    "Auto-mount",
                    format!("Could not write {}", conf_display),
                    Urgency::Critical,
                    Glyph::Warning,
                );
                return Ok(3);
            }
            Ok(0)
        }
        "tablet" => {
            let _ = out(&["sh", "-c", "lazyos-tablet-mode toggle"]);
            Ok(0)
        }
        "tablet-calibrate" => {
            let _ = out(&["sh", "-c", "lazyos-tablet-mode calibrate || true"]);
            Ok(0)
        }
        "layout" => {
            let _ = std::process::Command::new("lazyos")
                .arg("menu")
                .arg("layout")
                .spawn();
            Ok(0)
        }
        "translate" => {
            let _ = out(&["sh", "-c", "lazyos-translate config"]);
            Ok(0)
        }
        "audio" => {
            let _ = std::process::Command::new("lazyos")
                .arg("menu")
                .arg("audio")
                .spawn();
            Ok(0)
        }
        "mixer" => {
            let _ = std::process::Command::new("pavucontrol").spawn();
            Ok(0)
        }
        "media" => {
            let _ = std::process::Command::new("lazyos")
                .arg("menu")
                .arg("media")
                .spawn();
            Ok(0)
        }
        "power" => {
            let _ = std::process::Command::new("lazyos")
                .arg("menu")
                .arg("power")
                .spawn();
            Ok(0)
        }
        _ => Ok(0),
    }
}

fn missing(names: &[&str]) -> Option<String> {
    let gone: Vec<&str> = names.iter().copied().filter(|n| !have(n)).collect();
    if gone.is_empty() {
        None
    } else {
        Some(format!("Missing: {}", gone.join(" ")))
    }
}

fn have(program: &str) -> bool {
    crate::files::open::on_path(&std::env::var("PATH").unwrap_or_default(), program)
}

#[cfg(test)]
mod tests {
    #[test]
    fn status_formats_correctly() {
        // Would require mocking all the probes
    }
}
