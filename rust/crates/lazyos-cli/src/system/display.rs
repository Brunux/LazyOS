//! External display arrangement: position, resolution, scale (`rofi-display`).
//!
//! The monitor is placed relative to the internal panel (never moved).
//! A runtime arrangement is also written to `~/.config/sway/config.d/40-display.conf`
//! so it survives logout. Only the monitor's lines are written; the panel is
//! untouched because disabling it would boot a black screen.
//!
//! The blind `--panel-on` path runs from a key with no menu: it enables the
//! internal panel when both outputs are off.

use std::path::PathBuf;

use lazyos_ui::{Design, Glyph, Urgency};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use super::pick::{Picker, Question, row};
use super::{checked, out, print, say};

/// What the menu needs before it offers anything.
pub const NEEDED: &[&str] = &["swaymsg", "jq"];

/// The config file path.
fn config_path() -> PathBuf {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .and_then(|v| {
            let p = PathBuf::from(v);
            if p.is_absolute() { Some(p) } else { None }
        })
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("/"))
                .join(".config")
        });
    config_home.join("sway/config.d/40-display.conf")
}

/// The internal panel name (eDP-1 on this hardware).
const PANEL: &str = "eDP-1";

/// The command `--panel-on` runs to recover from a black screen.
pub fn panel_on_command() -> &'static str {
    "swaymsg output eDP-1 enable"
}

/// An output's live state from `swaymsg -t get_outputs`.
#[derive(Clone, Debug, Default, PartialEq)]
struct Output {
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale: f32,
    mode_width: u32,
    mode_height: u32,
    mode_refresh: u32,
    active: bool,
}

impl Output {
    fn mode_str(&self) -> String {
        if self.mode_width > 0 && self.mode_height > 0 {
            format!(
                "{}x{}@{}Hz",
                self.mode_width,
                self.mode_height,
                hz(self.mode_refresh)
            )
        } else {
            format!("{}x{}", self.width, self.height)
        }
    }
}

/// Parse `swaymsg -t get_outputs -r` JSON array into `Output` structs.
fn parse_outputs(json: &str) -> Vec<Output> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };
    let mut outputs = Vec::new();
    for v in items {
        let Some(name) = v["name"].as_str() else {
            continue;
        };
        let active = v["active"].as_bool().unwrap_or(false);
        let scale = v["scale"].as_f64().unwrap_or(1.0) as f32;
        let rect = &v["rect"];
        let x = rect["x"].as_i64().unwrap_or(0) as i32;
        let y = rect["y"].as_i64().unwrap_or(0) as i32;
        let width = rect["width"].as_u64().unwrap_or(0) as u32;
        let height = rect["height"].as_u64().unwrap_or(0) as u32;
        let cur = &v["current_mode"];
        let mode_width = cur["width"].as_u64().unwrap_or(0) as u32;
        let mode_height = cur["height"].as_u64().unwrap_or(0) as u32;
        let mode_refresh = cur["refresh"].as_u64().unwrap_or(0) as u32;
        outputs.push(Output {
            name: name.to_string(),
            x,
            y,
            width,
            height,
            scale,
            mode_width,
            mode_height,
            mode_refresh,
            active,
        });
    }
    outputs
}

/// Read all outputs from sway.
fn read_outputs() -> Vec<Output> {
    let output = out(&["swaymsg", "-t", "get_outputs", "-r"]).unwrap_or_default();
    parse_outputs(&output)
}

/// Find the external monitor (DP-*) and the internal panel (eDP-*).
fn find_monitor_and_panel(outputs: &[Output]) -> (Option<Output>, Option<Output>) {
    let mut mon = None;
    let mut panel = None;
    for o in outputs {
        if o.name.starts_with("DP-") {
            mon = Some(o.clone());
        } else if o.name.starts_with("eDP-") {
            panel = Some(o.clone());
        }
    }
    (mon, panel)
}

/// Convert mHz to Hz string.
fn hz(mhz: u32) -> String {
    let hz = mhz / 1000;
    if mhz.is_multiple_of(1000) {
        format!("{hz}")
    } else {
        format!("{}.{:03}", hz, mhz % 1000)
            .trim_end_matches('0')
            .to_string()
    }
}

/// Determine the arrangement name from positions.
fn arrangement(mon: &Output, panel: &Output) -> String {
    if !mon.active || !panel.active {
        return "".to_string();
    }
    if mon.y == panel.y && mon.x == panel.x {
        "mirror".to_string()
    } else if mon.y == panel.y && mon.x == panel.x + panel.width as i32 {
        "right".to_string()
    } else if mon.y == panel.y && mon.x + mon.width as i32 == panel.x {
        "left".to_string()
    } else if mon.x == panel.x && mon.y + mon.height as i32 == panel.y {
        "above".to_string()
    } else if mon.x == panel.x && mon.y == panel.y + panel.height as i32 {
        "below".to_string()
    } else {
        "custom".to_string()
    }
}

/// Save the current arrangement to the config file.
fn save_config(mon: &Output) -> Result<(), String> {
    if !mon.active {
        return Ok(());
    }
    let path = config_path();
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;

    let refresh_hz = hz(mon.mode_refresh);
    let mode = if mon.mode_width > 0 {
        format!("{}x{}@{}Hz", mon.mode_width, mon.mode_height, refresh_hz)
    } else {
        format!("{}x{}", mon.width, mon.height)
    };
    let scale = format!("{}", mon.scale);

    let content = format!(
        "# Written by lazyos menu display (Settings -> Display arrangement).\n\
         # The monitor is the output that can be absent, so a boot without it is\n\
         # untouched.  Delete this file to go back to sway default (on the right).\n\
         output {} resolution {}\n\
         output {} scale {}\n\
         output {} position {} {}\n",
        mon.name, mode, mon.name, scale, mon.name, mon.x, mon.y
    );

    let tmp = format!("{}.tmp.{}", path.display(), std::process::id());
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Parse available modes for the monitor from `swaymsg -t get_outputs -r` JSON.
fn parse_modes(json: &str, mon_name: &str) -> Vec<String> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(json)
    else {
        return Vec::new();
    };
    let mut modes = Vec::new();
    for v in items {
        if v["name"].as_str() == Some(mon_name) {
            if let Some(modes_arr) = v["modes"].as_array() {
                for m in modes_arr {
                    let w = m["width"].as_u64().unwrap_or(0);
                    let h = m["height"].as_u64().unwrap_or(0);
                    let r = m["refresh"].as_u64().unwrap_or(0);
                    let hz_str = hz(r as u32);
                    modes.push(format!("{}x{}@{}Hz", w, h, hz_str));
                }
            }
            break;
        }
    }
    modes.sort();
    modes.dedup();
    modes
}

/// Get available modes for the monitor.
fn get_modes(mon_name: &str) -> Vec<String> {
    let output = out(&["swaymsg", "-t", "get_outputs", "-r"]).unwrap_or_default();
    parse_modes(&output, mon_name)
}

/// The `--status` output (also used by the Settings row).
pub fn status() -> String {
    status_for(&read_outputs())
}

fn status_for(outputs: &[Output]) -> String {
    let (mon, panel) = find_monitor_and_panel(outputs);

    match (mon, panel) {
        (None, Some(p)) if !p.active => "internal panel off".to_string(),
        (None, _) => "no external monitor".to_string(),
        (Some(m), _) if !m.active => "monitor disabled".to_string(),
        (Some(_m), Some(p)) if !p.active => "monitor only".to_string(),
        (Some(m), Some(p)) => {
            let arr = arrangement(&m, &p);
            match arr.as_str() {
                "right" => "right of the panel".to_string(),
                "left" => "left of the panel".to_string(),
                "above" => "above the panel".to_string(),
                "below" => "below the panel".to_string(),
                "mirror" => "mirrored".to_string(),
                _ => "custom position".to_string(),
            }
        }
        _ => "unknown".to_string(),
    }
}

/// The `--panel-on` blind key handler.
pub fn panel_on() -> Result<(), String> {
    if let Err(e) = checked(&["swaymsg", "output", PANEL, "enable"]) {
        return Err(format!("Could not enable panel: {e}"));
    }
    print(&format!("{} is back on", PANEL));
    Ok(())
}

/// Build the `(key, label, detail)` rows for the current outputs.
fn rows_for(outputs: &[Output]) -> Vec<(String, String, String)> {
    let (mon, panel) = find_monitor_and_panel(outputs);
    let mut list = Vec::new();

    let Some(mon) = mon else {
        let panel_active = panel.as_ref().map(|p| p.active).unwrap_or(false);
        if !panel_active {
            list.push((
                "enable:panel".to_string(),
                "Bring the internal panel back".to_string(),
                format!("{} enable", PANEL),
            ));
        }
        list.push((
            "wd".to_string(),
            "Open wdisplays".to_string(),
            "arrange by dragging".to_string(),
        ));
        return list;
    };

    let panel = panel.unwrap_or_default();
    let cur_arr = arrangement(&mon, &panel);

    if mon.active && panel.active {
        for a in ["right", "left", "above", "below", "mirror"] {
            let (name, hint) = match a {
                "right" => ("Monitor on the right", "panel | monitor"),
                "left" => ("Monitor on the left", "monitor | panel"),
                "above" => ("Monitor above", "monitor over the panel"),
                "below" => ("Monitor below", "monitor under the panel"),
                "mirror" => ("Mirrored", "both show the same picture"),
                _ => continue,
            };
            let detail = if a == cur_arr { "current" } else { hint };
            list.push((format!("pos:{}", a), name.to_string(), detail.to_string()));
        }
    }

    if panel.active {
        list.push((
            "mono:monitor".to_string(),
            "Only the monitor".to_string(),
            "internal panel off · not remembered".to_string(),
        ));
    } else {
        list.push((
            "enable:panel".to_string(),
            "Bring the internal panel back".to_string(),
            format!("{} enable", PANEL),
        ));
    }
    if mon.active {
        list.push((
            "mono:panel".to_string(),
            "Only the internal panel".to_string(),
            "monitor off · not remembered".to_string(),
        ));
    } else {
        list.push((
            "enable:monitor".to_string(),
            "Bring the monitor back".to_string(),
            format!("{} enable", mon.name),
        ));
    }

    list.push((
        "res".to_string(),
        "Resolution".to_string(),
        format!("{} @ {} Hz", mon.mode_str(), hz(mon.mode_refresh)),
    ));
    list.push((
        "scale".to_string(),
        "Scale".to_string(),
        format!("{}", mon.scale),
    ));
    list.push((
        "wd".to_string(),
        "Open wdisplays".to_string(),
        "arrange by dragging".to_string(),
    ));
    list
}

/// The rows `lazyos menu display --rows` prints for `check-system.sh`.
pub fn rows() -> Vec<(String, String, String)> {
    rows_for(&read_outputs())
}

/// The resolution picker.
fn resolution_picker(mon: &Output) -> Picker {
    let modes = get_modes(&mon.name);
    let current = mon.mode_str();
    let rows: Vec<lazyos_ui::Row> = modes
        .iter()
        .map(|m| {
            let is_current = m == &current;
            row(
                if is_current {
                    format!("{} ✓", m)
                } else {
                    m.clone()
                },
                if is_current { "current" } else { "" },
                Glyph::Display,
            )
        })
        .collect();
    let keys = modes;

    Picker::new(
        "Resolution",
        vec![Question::choose(
            "Resolution",
            format!("{} · Enter applies · Escape closes", mon.name),
            rows,
            keys,
        )],
    )
}

/// The scale picker.
fn scale_picker(mon: &Output) -> Picker {
    let current = format!("{}", mon.scale);
    let scales = ["1", "1.25", "1.5", "1.75", "2"];
    let rows: Vec<lazyos_ui::Row> = scales
        .iter()
        .map(|s| {
            let is_current = s == &current;
            let detail: String = if is_current {
                "current".to_string()
            } else if *s == "1" || *s == "2" {
                "integer · crisp".to_string()
            } else {
                "fractional · slightly blurred".to_string()
            };
            row(format!("Scale {}", s), detail, Glyph::Display)
        })
        .collect();
    let keys: Vec<String> = scales.iter().map(|s| s.to_string()).collect();

    Picker::new(
        "Scale",
        vec![Question::choose(
            "Scale",
            format!("{} · Enter applies · Escape closes", mon.name),
            rows,
            keys,
        )],
    )
}

/// Run a picker and handle errors.
fn run_picker(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
    name: &str,
    picker: &mut Picker,
) -> Result<i32, crate::Failure> {
    let code = crate::run_screen(terminal, design, name, picker)?;
    Ok(code)
}

/// Apply arrangement and save.
fn apply_and_save(arr: &str, mon: &Output, panel: &Output) -> Result<(), String> {
    let (x, y) = match arr {
        "right" => (panel.x + panel.width as i32, panel.y),
        "left" => (panel.x - mon.width as i32, panel.y),
        "above" => (panel.x, panel.y - mon.height as i32),
        "below" => (panel.x, panel.y + panel.height as i32),
        "mirror" => (panel.x, panel.y),
        _ => return Err("unknown arrangement".to_string()),
    };
    if let Err(e) = checked(&[
        "swaymsg",
        "--",
        "output",
        &mon.name,
        "position",
        &x.to_string(),
        &y.to_string(),
    ]) {
        return Err(format!("Could not position monitor: {e}"));
    }
    save_config(mon)?;
    Ok(())
}

/// The main display menu.
fn display_menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    if let Some(said) = missing(NEEDED) {
        say(
            "Display",
            format!("{said} (sway, jq)"),
            Urgency::Critical,
            Glyph::Display,
        );
        return Ok(3);
    }

    let outputs = read_outputs();
    let (mon, panel) = find_monitor_and_panel(&outputs);

    if mon.is_none() {
        // No external monitor
        let entries = rows_for(&outputs);
        let rows: Vec<lazyos_ui::Row> = entries
            .iter()
            .map(|(k, l, d)| {
                let g = if k == "wd" {
                    Glyph::Mouse
                } else {
                    Glyph::Desktop
                };
                row(l.clone(), d.clone(), g)
            })
            .collect();
        let keys: Vec<String> = entries.into_iter().map(|(k, _, _)| k).collect();
        let mut picker = Picker::new(
            "Display",
            vec![Question::choose(
                "Display",
                "No external monitor · plug one into a USB-C port",
                rows,
                keys,
            )],
        );
        let code = run_picker(terminal, design, "Display", &mut picker)?;
        if code != 0 {
            return Ok(code);
        }
        let answers = picker.answers();
        if let Some(key) = answers.first() {
            match key.as_str() {
                "enable:panel" => {
                    if let Err(e) = checked(&["swaymsg", "output", PANEL, "enable"]) {
                        say(
                            "Display",
                            format!("Could not enable panel: {e}"),
                            Urgency::Critical,
                            Glyph::Display,
                        );
                        return Ok(3);
                    }
                }
                "wd" => {
                    if !have("wdisplays") {
                        say(
                            "Display",
                            "wdisplays is not installed (apt install wdisplays)".to_string(),
                            Urgency::Critical,
                            Glyph::Display,
                        );
                        return Ok(3);
                    }
                    if out(&["pgrep", "-x", "wdisplays"]).is_some() {
                        let _ = out(&["swaymsg", "[app_id=\"wdisplays\"]", "focus"]);
                        return Ok(0);
                    }
                    let _ = out(&["swaymsg", "workspace", "number", "11"]);
                    std::process::Command::new("wdisplays").spawn().ok();
                }
                _ => {}
            }
        }
        return Ok(0);
    }

    let mon = mon.unwrap();
    let panel = panel.unwrap_or_default();
    let cur_arr = arrangement(&mon, &panel);

    let mut rows = Vec::new();
    let mut keys = Vec::new();

    if mon.active && panel.active {
        rows.push(row("Arrangement", "where the monitor sits", Glyph::Display));
        keys.push("-".to_string());
        for a in ["right", "left", "above", "below", "mirror"] {
            let (name, hint) = match a {
                "right" => ("Monitor on the right", "panel | monitor"),
                "left" => ("Monitor on the left", "monitor | panel"),
                "above" => ("Monitor above", "monitor over the panel"),
                "below" => ("Monitor below", "monitor under the panel"),
                "mirror" => ("Mirrored", "both show the same picture"),
                _ => continue,
            };
            if a == cur_arr {
                rows.push(row(name, "current", Glyph::Display));
            } else {
                rows.push(row(name, hint, Glyph::Display));
            }
            keys.push(format!("pos:{}", a));
        }
    }

    rows.push(row("One screen", "", Glyph::Desktop));
    keys.push("-".to_string());
    if panel.active {
        rows.push(row(
            "Only the monitor",
            "internal panel off · not remembered",
            Glyph::Desktop,
        ));
        keys.push("mono:monitor".to_string());
    } else {
        rows.push(row(
            "Bring the internal panel back",
            format!("{} enable", PANEL),
            Glyph::Desktop,
        ));
        keys.push("enable:panel".to_string());
    }
    if mon.active {
        rows.push(row(
            "Only the internal panel",
            "monitor off · not remembered",
            Glyph::Desktop,
        ));
        keys.push("mono:panel".to_string());
    } else {
        rows.push(row(
            "Bring the monitor back",
            format!("{} enable", mon.name),
            Glyph::Desktop,
        ));
        keys.push("enable:monitor".to_string());
    }

    rows.push(row("The monitor", "", Glyph::Display));
    keys.push("-".to_string());
    rows.push(row(
        "Resolution",
        format!("{} @ {} Hz", mon.mode_str(), hz(mon.mode_refresh)),
        Glyph::Display,
    ));
    rows.push(row("Scale", format!("{}", mon.scale), Glyph::Display));
    keys.push("res".to_string());
    keys.push("scale".to_string());

    rows.push(row("Manual", "", Glyph::Mouse));
    keys.push("-".to_string());
    rows.push(row("Open wdisplays", "arrange by dragging", Glyph::Mouse));
    keys.push("wd".to_string());

    rows.push(row(
        "Saved for next login",
        config_path().display().to_string(),
        Glyph::Gear,
    ));
    keys.push("-".to_string());

    let mut picker = Picker::new(
        "Display",
        vec![Question::choose(
            "Display",
            "Enter applies · Escape closes",
            rows,
            keys,
        )],
    );

    let code = run_picker(terminal, design, "Display", &mut picker)?;
    if code != 0 {
        return Ok(code);
    }

    let answers = picker.answers();
    if answers.is_empty() {
        return Ok(1);
    }
    let key = &answers[0];

    match key.as_str() {
        k if k.starts_with("pos:") => {
            let arr = &k[4..];
            let outputs = read_outputs();
            if let (Some(m), Some(p)) = find_monitor_and_panel(&outputs)
                && let Err(e) = apply_and_save(arr, &m, &p)
            {
                say(
                    "Display",
                    format!("Could not apply arrangement: {e}"),
                    Urgency::Critical,
                    Glyph::Display,
                );
                return Ok(3);
            }
            Ok(0)
        }
        "res" => {
            let mut picker = resolution_picker(&mon);
            let code = run_picker(terminal, design, "Resolution", &mut picker)?;
            if code != 0 {
                return Ok(code);
            }
            if let Some(mode) = picker.answers().first() {
                if let Err(e) = checked(&["swaymsg", "output", &mon.name, "resolution", mode]) {
                    say(
                        "Display",
                        format!("Could not set resolution: {e}"),
                        Urgency::Critical,
                        Glyph::Display,
                    );
                    return Ok(3);
                }
                let outputs = read_outputs();
                if let (Some(m), Some(p)) = find_monitor_and_panel(&outputs) {
                    let cur = arrangement(&m, &p);
                    if ["right", "left", "above", "below", "mirror"].contains(&cur.as_str())
                        && let Err(e) = apply_and_save(&cur, &m, &p)
                    {
                        say(
                            "Display",
                            format!("Could not re-apply arrangement: {e}"),
                            Urgency::Critical,
                            Glyph::Display,
                        );
                        return Ok(3);
                    }
                }
            }
            Ok(0)
        }
        "scale" => {
            let mut picker = scale_picker(&mon);
            let code = run_picker(terminal, design, "Scale", &mut picker)?;
            if code != 0 {
                return Ok(code);
            }
            if let Some(scale) = picker.answers().first() {
                if let Err(e) = checked(&["swaymsg", "output", &mon.name, "scale", scale]) {
                    say(
                        "Display",
                        format!("Could not set scale: {e}"),
                        Urgency::Critical,
                        Glyph::Display,
                    );
                    return Ok(3);
                }
                let outputs = read_outputs();
                if let (Some(m), Some(p)) = find_monitor_and_panel(&outputs) {
                    let cur = arrangement(&m, &p);
                    if ["right", "left", "above", "below", "mirror"].contains(&cur.as_str())
                        && let Err(e) = apply_and_save(&cur, &m, &p)
                    {
                        say(
                            "Display",
                            format!("Could not re-apply arrangement: {e}"),
                            Urgency::Critical,
                            Glyph::Display,
                        );
                        return Ok(3);
                    }
                }
            }
            Ok(0)
        }
        "wd" => {
            if !have("wdisplays") {
                say(
                    "Display",
                    "wdisplays is not installed (apt install wdisplays)".to_string(),
                    Urgency::Critical,
                    Glyph::Display,
                );
                return Ok(3);
            }
            if out(&["pgrep", "-x", "wdisplays"]).is_some() {
                let _ = out(&["swaymsg", "[app_id=\"wdisplays\"]", "focus"]);
                return Ok(0);
            }
            let _ = out(&["swaymsg", "workspace", "number", "11"]);
            std::process::Command::new("wdisplays").spawn().ok();
            Ok(0)
        }
        "enable:panel" => {
            if let Err(e) = checked(&["swaymsg", "output", PANEL, "enable"]) {
                say(
                    "Display",
                    format!("Could not enable panel: {e}"),
                    Urgency::Critical,
                    Glyph::Display,
                );
            }
            Ok(0)
        }
        "enable:monitor" => {
            if let Some(m) = read_outputs().iter().find(|o| o.name.starts_with("DP-"))
                && let Err(e) = checked(&["swaymsg", "output", &m.name, "enable"])
            {
                say(
                    "Display",
                    format!("Could not enable monitor: {e}"),
                    Urgency::Critical,
                    Glyph::Display,
                );
            }
            Ok(0)
        }
        "mono:monitor" => {
            if let Err(e) = checked(&["swaymsg", "output", PANEL, "disable"]) {
                say(
                    "Display",
                    format!("Could not disable panel: {e}"),
                    Urgency::Critical,
                    Glyph::Display,
                );
            } else {
                say(
                    "Display",
                    format!(
                        "Only {}. Bring the panel back with: swaymsg output {} enable",
                        mon.name, PANEL
                    ),
                    Urgency::Normal,
                    Glyph::Display,
                );
            }
            Ok(0)
        }
        "mono:panel" => {
            if let Err(e) = checked(&["swaymsg", "output", &mon.name, "disable"]) {
                say(
                    "Display",
                    format!("Could not disable monitor: {e}"),
                    Urgency::Critical,
                    Glyph::Display,
                );
            } else {
                say(
                    "Display",
                    format!(
                        "Only {}. Bring the monitor back with: swaymsg output {} enable",
                        PANEL, mon.name
                    ),
                    Urgency::Normal,
                    Glyph::Display,
                );
            }
            Ok(0)
        }
        _ => Ok(0),
    }
}

/// `lazyos menu display` (`rofi-display`): the main menu plus sub-menus.
pub fn menu(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    design: &Design,
) -> Result<i32, crate::Failure> {
    display_menu(terminal, design)
}

pub fn missing(names: &[&str]) -> Option<String> {
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
    use super::*;

    #[test]
    fn hz_formats_correctly() {
        assert_eq!(hz(60000), "60");
        assert_eq!(hz(59940), "59.94");
        assert_eq!(hz(120000), "120");
    }

    #[test]
    fn arrangement_detects_positions() {
        let panel = Output {
            x: 0,
            y: 0,
            width: 1366,
            height: 912,
            active: true,
            ..Default::default()
        };
        let mon = Output {
            x: 1366,
            y: 0,
            width: 1920,
            height: 1080,
            active: true,
            ..Default::default()
        };
        assert_eq!(arrangement(&mon, &panel), "right");

        let mon = Output {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
            active: true,
            ..Default::default()
        };
        assert_eq!(arrangement(&mon, &panel), "left");

        let mon = Output {
            x: 0,
            y: -1080,
            width: 1920,
            height: 1080,
            active: true,
            ..Default::default()
        };
        assert_eq!(arrangement(&mon, &panel), "above");

        let mon = Output {
            x: 0,
            y: 912,
            width: 1920,
            height: 1080,
            active: true,
            ..Default::default()
        };
        assert_eq!(arrangement(&mon, &panel), "below");

        let mon = Output {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            active: true,
            ..Default::default()
        };
        assert_eq!(arrangement(&mon, &panel), "mirror");
    }

    #[test]
    fn panel_off_with_no_monitor_offers_recovery_row() {
        let json = r#"[{"name":"eDP-1","active":false,"scale":1.0,"rect":{"x":0,"y":0,"width":0,"height":0},"current_mode":null,"modes":[]}]"#;
        let outputs = parse_outputs(json);
        assert_eq!(outputs.len(), 1);
        assert!(!outputs[0].active);
        assert_eq!(status_for(&outputs), "internal panel off");
        let rows = rows_for(&outputs);
        assert!(
            rows.iter()
                .any(|(k, l, _)| k == "enable:panel" && l == "Bring the internal panel back")
        );
    }

    #[test]
    fn parse_swaymsg_outputs_and_modes() {
        let json = r#"[
          {"name":"eDP-1","active":true,"scale":1.0,"rect":{"x":0,"y":0,"width":1366,"height":768},"current_mode":{"width":1366,"height":768,"refresh":60000},"modes":[]},
          {"name":"DP-1","active":true,"scale":1.25,"rect":{"x":1366,"y":0,"width":1920,"height":1080},"current_mode":{"width":1920,"height":1080,"refresh":59940},"modes":[{"width":1920,"height":1080,"refresh":60000}]}
        ]"#;
        let outputs = parse_outputs(json);
        assert_eq!(outputs.len(), 2);
        assert_eq!(status_for(&outputs), "right of the panel");
        let modes = parse_modes(json, "DP-1");
        assert_eq!(modes, vec!["1920x1080@60Hz"]);
    }
}
